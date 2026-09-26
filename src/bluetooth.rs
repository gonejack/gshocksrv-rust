use crate::gshock::watch::{Watch, WatchError, WatchIo};
use crate::gshock::{self, Button};
use crate::server::{BluetoothBackend, ConnectedWatch};
use anyhow::{Context, Result, anyhow, bail};
use btleplug::api::{BDAddr, Central, CharPropFlags, Manager as _, Peripheral as _, ScanFilter, WriteType};
use btleplug::platform::{Adapter, Manager, Peripheral};
use chrono::Local;
use futures_util::StreamExt;
use std::future::Future;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use uuid::Uuid;

pub struct BtleplugBackend {
    runtime: Arc<Runtime>,
    adapter: Adapter,
    stop: Arc<AtomicBool>,
}

impl BtleplugBackend {
    pub fn new() -> Result<Self> {
        Self::with_stop(Arc::new(AtomicBool::new(false)))
    }

    pub fn with_stop(stop: Arc<AtomicBool>) -> Result<Self> {
        let runtime = Arc::new(Runtime::new().context("create Tokio runtime")?);
        let manager = runtime.block_on(Manager::new()).context("create BLE manager")?;
        let adapter = runtime
            .block_on(manager.adapters())
            .context("list BLE adapters")?
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("no Bluetooth adapter found"))?;
        Ok(Self { runtime, adapter, stop })
    }
}

impl BluetoothBackend for BtleplugBackend {
    fn scan_and_connect(&mut self, timeout: Duration, accept: &dyn Fn(&str) -> bool, stop: &dyn Fn() -> bool) -> Result<Box<dyn ConnectedWatch>> {
        let runtime = Arc::clone(&self.runtime);
        let task_runtime = Arc::clone(&runtime);
        let adapter = self.adapter.clone();
        let stopped = Arc::clone(&self.stop);
        runtime.block_on(async move {
            let service = Uuid::parse_str(gshock::CASIO_SERVICE_UUID).context("parse Casio service UUID")?;
            adapter.start_scan(ScanFilter { services: vec![service] }).await.context("start BLE scan")?;
            let deadline = tokio::time::Instant::now() + timeout;
            let found = loop {
                if stop() {
                    let _ = adapter.stop_scan().await;
                    bail!("scan interrupted");
                }
                let peripherals = adapter.peripherals().await.context("list BLE devices")?;
                let mut found = None;
                for peripheral in peripherals {
                    let properties = peripheral.properties().await.context("read BLE properties")?;
                    let Some(properties) = properties else { continue };
                    let name = properties.local_name.unwrap_or_default();
                    if accept(&name) {
                        found = Some((peripheral, name));
                        break;
                    }
                }
                if found.is_some() {
                    break found;
                }
                if tokio::time::Instant::now() >= deadline {
                    break None;
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            };
            let _ = adapter.stop_scan().await;
            let Some((peripheral, name)) = found else {
                bail!("no matching watch found");
            };
            peripheral.connect().await.with_context(|| format!("connect to {name}"))?;
            let io = BtleIo::connect(Arc::clone(&task_runtime), peripheral.clone(), stopped).await?;
            let identifier = peripheral_identifier(&peripheral);
            let profile = gshock::profile_for(&name);
            let watch = Watch { name: name.clone(), identifier, profile, io, request_timeout: Duration::from_secs(5) };
            Ok(Box::new(BtleConnectedWatch { peripheral, watch }) as Box<dyn ConnectedWatch>)
        })
    }
}

struct BtleConnectedWatch {
    peripheral: Peripheral,
    watch: Watch<BtleIo>,
}

impl ConnectedWatch for BtleConnectedWatch {
    fn name(&self) -> &str {
        &self.watch.name
    }

    fn identifier(&self) -> &str {
        &self.watch.identifier
    }

    fn always_connected(&self) -> bool {
        self.watch.profile.always_connected
    }

    fn pressed_button(&mut self, timeout: Duration) -> Result<Button> {
        self.watch.request_timeout = timeout;
        self.watch.io.operation_timeout = timeout;
        self.watch.pressed_button().context("read button from watch")
    }

    fn set_time(&mut self, adjustment_secs: i64, timeout: Duration) -> Result<chrono::DateTime<Local>> {
        self.watch.request_timeout = timeout;
        self.watch.io.operation_timeout = timeout;
        self.watch.set_time(adjustment_secs).context("set watch time")
    }

    fn disconnect(&mut self) -> Result<()> {
        self.watch.io.run_ble("disconnect", self.peripheral.disconnect()).context("disconnect")?;
        Ok(())
    }
}

fn peripheral_identifier(peripheral: &Peripheral) -> String {
    let addr = peripheral.address();
    if addr == BDAddr::default() {
        format!("id:{}", peripheral.id())
    } else {
        format!("mac:{addr}")
    }
}

struct BtleIo {
    runtime: Arc<Runtime>,
    peripheral: Peripheral,
    read_request: btleplug::api::Characteristic,
    all_features: btleplug::api::Characteristic,
    sp_request: Option<btleplug::api::Characteristic>,
    sp_data: Option<btleplug::api::Characteristic>,
    notifications: mpsc::Receiver<Vec<u8>>,
    sp_notifications: mpsc::Receiver<Vec<u8>>,
    operation_timeout: Duration,
    stop: Arc<AtomicBool>,
}

impl BtleIo {
    async fn connect(runtime: Arc<Runtime>, peripheral: Peripheral, stop: Arc<AtomicBool>) -> Result<Self> {
        peripheral.discover_services().await.context("discover services")?;
        let characteristics = peripheral.characteristics();
        let find = |uuid: &str| {
            let wanted = Uuid::parse_str(uuid).ok()?;
            characteristics.iter().find(|characteristic| characteristic.uuid == wanted).cloned()
        };
        let read_request = find(gshock::READ_REQUEST_UUID).ok_or_else(|| anyhow!("watch lacks read-request characteristic"))?;
        let all_features = find(gshock::ALL_FEATURES_UUID).ok_or_else(|| anyhow!("watch lacks all-features characteristic"))?;
        let sp_request = find(gshock::SP_REQUEST_UUID);
        let sp_data = find(gshock::SP_DATA_UUID);
        for characteristic in &characteristics {
            if characteristic.properties.intersects(CharPropFlags::NOTIFY | CharPropFlags::INDICATE) {
                peripheral.subscribe(characteristic).await.with_context(|| format!("subscribe {}", characteristic.uuid))?;
            }
        }
        let (tx, rx) = mpsc::sync_channel(64);
        let (sp_tx, sp_rx) = mpsc::sync_channel(64);
        let mut stream = peripheral.notifications().await.context("open notification stream")?;
        let sp_uuid = sp_data.as_ref().map(|characteristic| characteristic.uuid);
        tokio::spawn(async move {
            while let Some(notification) = stream.next().await {
                let result = if Some(notification.uuid) == sp_uuid {
                    sp_tx.try_send(notification.value)
                } else {
                    tx.try_send(notification.value)
                };
                if matches!(result, Err(mpsc::TrySendError::Disconnected(_))) {
                    break;
                }
            }
        });
        Ok(Self {
            runtime,
            peripheral,
            read_request,
            all_features,
            sp_request,
            sp_data,
            notifications: rx,
            sp_notifications: sp_rx,
            operation_timeout: Duration::from_secs(5),
            stop,
        })
    }

    fn receive(channel: &mpsc::Receiver<Vec<u8>>, timeout: Duration, stop: &AtomicBool) -> Result<Vec<u8>, WatchError> {
        let started = Instant::now();
        loop {
            if stop.load(Ordering::Relaxed) {
                return Err(WatchError::Interrupted);
            }
            let remaining = timeout.checked_sub(started.elapsed()).ok_or(WatchError::Timeout)?;
            match channel.recv_timeout(remaining.min(Duration::from_millis(100))) {
                Ok(data) => return Ok(data),
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => return Err(WatchError::Transport("notification stream closed".into())),
            }
        }
    }

    fn run_ble<T, E>(&self, operation: &str, future: impl Future<Output = Result<T, E>>) -> Result<T, WatchError>
    where
        E: std::fmt::Display,
    {
        self.runtime.block_on(ble_operation(&self.stop, self.operation_timeout, operation, future))
    }
}

async fn ble_operation<T, E>(stop: &AtomicBool, timeout: Duration, operation: &str, future: impl Future<Output = Result<T, E>>) -> Result<T, WatchError>
where
    E: std::fmt::Display,
{
    tokio::select! {
        biased;
        _ = async {
            while !stop.load(Ordering::Relaxed) {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        } => Err(WatchError::Interrupted),
        result = tokio::time::timeout(timeout, future) => match result {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error)) => Err(WatchError::Transport(format!("{operation}: {error}"))),
            Err(_) => Err(WatchError::Transport(format!("{operation} timed out after {timeout:?}"))),
        },
    }
}

impl WatchIo for BtleIo {
    fn write(&mut self, data: &[u8], without_response: bool) -> Result<(), WatchError> {
        let characteristic = if without_response { &self.read_request } else { &self.all_features };
        let write_type = if without_response { WriteType::WithoutResponse } else { WriteType::WithResponse };
        let operation = format!("write GATT characteristic {} feature 0x{:02x}", characteristic.uuid, data.first().copied().unwrap_or_default());
        self.run_ble(&operation, self.peripheral.write(characteristic, data, write_type))
    }

    fn read_response(&mut self, expected: u8, timeout: Duration, analogue: bool) -> Result<Vec<u8>, WatchError> {
        let started = Instant::now();
        loop {
            let remaining = timeout.checked_sub(started.elapsed()).ok_or(WatchError::Timeout)?;
            let data = Self::receive(&self.notifications, remaining, &self.stop)?;
            if gshock::protocol::response_key(&data, analogue) == Some(expected) {
                return Ok(gshock::protocol::unwrap_response(&data, expected, analogue).to_vec());
            }
        }
    }

    fn request_sp(&mut self, request: &[u8], expected_len: usize, timeout: Duration) -> Result<Vec<u8>, WatchError> {
        let characteristic = self.sp_request.as_ref().ok_or(WatchError::MissingMip)?;
        self.run_ble("write SP request", self.peripheral.write(characteristic, request, WriteType::WithoutResponse))?;
        let started = Instant::now();
        let mut data = Vec::with_capacity(expected_len);
        while data.len() < expected_len {
            let remaining = timeout.checked_sub(started.elapsed()).ok_or(WatchError::Timeout)?;
            data.extend(Self::receive(&self.sp_notifications, remaining, &self.stop)?);
        }
        Ok(data)
    }

    fn write_sp(&mut self, data: &[u8]) -> Result<(), WatchError> {
        let characteristic = self.sp_data.as_ref().ok_or(WatchError::MissingMip)?;
        self.run_ble("write SP data", self.peripheral.write(characteristic, data, WriteType::WithResponse))
    }

    fn is_connected(&self) -> Result<bool, WatchError> {
        self.run_ble("check BLE connection", self.peripheral.is_connected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stalled_ble_operation_times_out_or_stops() {
        let runtime = Runtime::new().unwrap();
        let stopped = Arc::new(AtomicBool::new(false));
        let pending = || std::future::pending::<std::io::Result<()>>();

        let result = runtime.block_on(ble_operation(&stopped, Duration::from_millis(20), "test write", pending()));
        assert!(matches!(result, Err(WatchError::Transport(message)) if message.contains("test write timed out")));

        let signal = Arc::clone(&stopped);
        let worker = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            signal.store(true, Ordering::Relaxed);
        });
        let result = runtime.block_on(ble_operation(&stopped, Duration::from_secs(2), "test write", pending()));
        worker.join().unwrap();
        assert!(matches!(result, Err(WatchError::Interrupted)));
    }

    #[test]
    fn notification_wait_stops() {
        let (sender, receiver) = mpsc::channel();
        let stopped = Arc::new(AtomicBool::new(false));
        let signal = Arc::clone(&stopped);
        let worker = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            signal.store(true, Ordering::Relaxed);
        });

        let result = BtleIo::receive(&receiver, Duration::from_secs(2), &stopped);
        worker.join().unwrap();
        drop(sender);
        assert!(matches!(result, Err(WatchError::Interrupted)));
    }
}
