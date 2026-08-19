use crate::gshock::watch::{Watch, WatchError, WatchIo};
use crate::gshock::{self, Button};
use crate::server::{BluetoothBackend, ConnectedWatch};
use btleplug::api::{Central, CharPropFlags, Manager as _, Peripheral as _, ScanFilter, WriteType};
use btleplug::platform::{Adapter, Manager, Peripheral};
use chrono::Local;
use futures_util::StreamExt;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use uuid::Uuid;

pub struct BtleplugBackend {
    runtime: Arc<Runtime>,
    adapter: Adapter,
}

impl BtleplugBackend {
    pub fn new() -> Result<Self, String> {
        let runtime = Arc::new(Runtime::new().map_err(|e| format!("create Tokio runtime: {e}"))?);
        let manager = runtime.block_on(Manager::new()).map_err(|e| format!("create BLE manager: {e}"))?;
        let adapter = runtime
            .block_on(manager.adapters())
            .map_err(|e| format!("list BLE adapters: {e}"))?
            .into_iter()
            .next()
            .ok_or_else(|| "no Bluetooth adapter found".to_string())?;
        Ok(Self { runtime, adapter })
    }
}

impl BluetoothBackend for BtleplugBackend {
    fn scan_and_connect(&mut self, timeout: Duration, accept: &dyn Fn(&str) -> bool) -> Result<Box<dyn ConnectedWatch>, String> {
        let runtime = Arc::clone(&self.runtime);
        let task_runtime = Arc::clone(&runtime);
        let adapter = self.adapter.clone();
        runtime.block_on(async move {
            let service = Uuid::parse_str(gshock::CASIO_SERVICE_UUID).map_err(|e| format!("parse Casio service UUID: {e}"))?;
            adapter.start_scan(ScanFilter { services: vec![service] }).await.map_err(|e| format!("start BLE scan: {e}"))?;
            let deadline = tokio::time::Instant::now() + timeout;
            let found = loop {
                let peripherals = adapter.peripherals().await.map_err(|e| format!("list BLE devices: {e}"))?;
                let mut found = None;
                for peripheral in peripherals {
                    let properties = peripheral.properties().await.map_err(|e| format!("read BLE properties: {e}"))?;
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
            let Some((peripheral, name)) = found else { return Err("no matching watch found".into()) };
            peripheral.connect().await.map_err(|e| format!("connect to {name}: {e}"))?;
            let io = BtleIo::connect(Arc::clone(&task_runtime), peripheral.clone()).await?;
            let address = peripheral.address().to_string();
            let profile = gshock::profile_for(&name);
            let watch = Watch { name: name.clone(), address: address.clone(), profile, io, request_timeout: Duration::from_secs(5) };
            Ok(Box::new(BtleConnectedWatch { name, address, always_connected: profile.always_connected, peripheral, runtime: task_runtime, watch })
                as Box<dyn ConnectedWatch>)
        })
    }
}

struct BtleConnectedWatch {
    name: String,
    address: String,
    always_connected: bool,
    peripheral: Peripheral,
    runtime: Arc<Runtime>,
    watch: Watch<BtleIo>,
}
impl ConnectedWatch for BtleConnectedWatch {
    fn name(&self) -> &str {
        &self.name
    }
    fn address(&self) -> &str {
        &self.address
    }
    fn always_connected(&self) -> bool {
        self.always_connected
    }
    fn pressed_button(&mut self, timeout: Duration) -> Result<Button, String> {
        self.watch.request_timeout = timeout;
        self.watch.pressed_button().map_err(|e| e.to_string())
    }
    fn set_time(&mut self, adjustment_secs: i64, timeout: Duration) -> Result<chrono::DateTime<Local>, String> {
        self.watch.request_timeout = timeout;
        self.watch.set_time(adjustment_secs).map_err(|e| e.to_string())
    }
    fn disconnect(&mut self) -> Result<(), String> {
        self.runtime.block_on(self.peripheral.disconnect()).map_err(|e| format!("disconnect: {e}"))
    }
}

struct BtleIo {
    runtime: Arc<Runtime>,
    peripheral: Peripheral,
    read_request: btleplug::api::Characteristic,
    all_features: btleplug::api::Characteristic,
    sp_request: Option<btleplug::api::Characteristic>,
    sp_data: Option<btleplug::api::Characteristic>,
    notifications: Arc<Mutex<mpsc::Receiver<Vec<u8>>>>,
    sp_notifications: Arc<Mutex<mpsc::Receiver<Vec<u8>>>>,
}
impl BtleIo {
    async fn connect(runtime: Arc<Runtime>, peripheral: Peripheral) -> Result<Self, String> {
        peripheral.discover_services().await.map_err(|e| format!("discover services: {e}"))?;
        let characteristics = peripheral.characteristics();
        let find = |uuid: &str| {
            let wanted = Uuid::parse_str(uuid).ok()?;
            characteristics.iter().find(|c| c.uuid == wanted).cloned()
        };
        let read_request = find(gshock::READ_REQUEST_UUID).ok_or_else(|| "watch lacks read-request characteristic".to_string())?;
        let all_features = find(gshock::ALL_FEATURES_UUID).ok_or_else(|| "watch lacks all-features characteristic".to_string())?;
        let sp_request = find(gshock::SP_REQUEST_UUID);
        let sp_data = find(gshock::SP_DATA_UUID);
        for characteristic in &characteristics {
            if characteristic.properties.intersects(CharPropFlags::NOTIFY | CharPropFlags::INDICATE) {
                peripheral.subscribe(characteristic).await.map_err(|e| format!("subscribe {}: {e}", characteristic.uuid))?;
            }
        }
        let (tx, rx) = mpsc::channel();
        let (sp_tx, sp_rx) = mpsc::channel();
        let mut stream = peripheral.notifications().await.map_err(|e| format!("open notification stream: {e}"))?;
        let sp_uuid = sp_data.as_ref().map(|c| c.uuid);
        tokio::spawn(async move {
            while let Some(notification) = stream.next().await {
                let result = if Some(notification.uuid) == sp_uuid { sp_tx.send(notification.value) } else { tx.send(notification.value) };
                if result.is_err() {
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
            notifications: Arc::new(Mutex::new(rx)),
            sp_notifications: Arc::new(Mutex::new(sp_rx)),
        })
    }
    fn receive(&self, channel: &Arc<Mutex<mpsc::Receiver<Vec<u8>>>>, timeout: Duration) -> Result<Vec<u8>, WatchError> {
        channel
            .lock()
            .map_err(|_| WatchError::Transport("notification queue poisoned".into()))?
            .recv_timeout(timeout)
            .map_err(|_| WatchError::Timeout)
    }
}
impl WatchIo for BtleIo {
    fn write(&mut self, data: &[u8], without_response: bool) -> Result<(), WatchError> {
        let characteristic = if without_response { &self.read_request } else { &self.all_features };
        let write_type = if without_response { WriteType::WithoutResponse } else { WriteType::WithResponse };
        self.runtime
            .block_on(self.peripheral.write(characteristic, data, write_type))
            .map_err(|e| WatchError::Transport(format!("write GATT characteristic: {e}")))
    }
    fn read_response(&mut self, expected: u8, timeout: Duration, analogue: bool) -> Result<Vec<u8>, WatchError> {
        let started = Instant::now();
        loop {
            let remaining = timeout.checked_sub(started.elapsed()).ok_or(WatchError::Timeout)?;
            let data = self.receive(&self.notifications, remaining)?;
            if gshock::protocol::response_key(&data, analogue) == Some(expected) {
                return Ok(gshock::protocol::unwrap_response(&data, expected, analogue).to_vec());
            }
        }
    }
    fn request_sp(&mut self, request: &[u8], expected_len: usize, timeout: Duration) -> Result<Vec<u8>, WatchError> {
        let characteristic = self.sp_request.as_ref().ok_or(WatchError::MissingMip)?;
        self.runtime
            .block_on(self.peripheral.write(characteristic, request, WriteType::WithoutResponse))
            .map_err(|e| WatchError::Transport(format!("write SP request: {e}")))?;
        let started = Instant::now();
        let mut data = Vec::with_capacity(expected_len);
        while data.len() < expected_len {
            let remaining = timeout.checked_sub(started.elapsed()).ok_or(WatchError::Timeout)?;
            data.extend(self.receive(&self.sp_notifications, remaining)?);
        }
        Ok(data)
    }
    fn write_sp(&mut self, data: &[u8]) -> Result<(), WatchError> {
        let characteristic = self.sp_data.as_ref().ok_or(WatchError::MissingMip)?;
        self.runtime
            .block_on(self.peripheral.write(characteristic, data, WriteType::WithResponse))
            .map_err(|e| WatchError::Transport(format!("write SP data: {e}")))
    }
}
