#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use embassy_futures::select::Either;
use embassy_time::{Duration, Timer};
use embassy_executor::Spawner;
use embassy_net::{
    IpListenEndpoint,
    Runner,
    StackResources,
    dns::DnsSocket,
    tcp::{
        TcpSocket,
        client::{TcpClient, TcpClientState},
    },
};

use esp_hal::{
    clock::CpuClock,
    ram,
    timer::timg::TimerGroup,
    rng::Rng,
};

use esp_println::{println, print};
use esp_radio::wifi::{
    AuthenticationMethodConfig, Config, ControllerConfig, Interface, WifiController, ap::AccessPointConfig, sta::StationConfig,
};

use crate::host::tcp::TcpChannel;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("Panic!: {:?}", info);
    loop {}
}

macro_rules! mk_static {
    ($t:ty,$val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        #[deny(unused_attributes)]
        let x = STATIC_CELL.uninit().write(($val));
        x
    }};
}

mod probe;
mod host;
mod cmd;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

const SSID: &str = env!("SSID");
const PASSWORD: &str = env!("PASSWORD");

const DAP_PORT: u16 = 8080;

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_hal::main]
async fn main(spawner: Spawner) -> ! {

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(#[ram(reclaimed)] size: 64 * 1024);
    esp_alloc::heap_allocator!(size: 36 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    let station_config = Config::Station(
        StationConfig::default()
            .with_ssid(SSID.try_into().unwrap())
            .with_authentication(AuthenticationMethodConfig::Wpa2Personal(
                PASSWORD.try_into().unwrap(),
            )),
    );

    println!("linking wifi...");
    let wifi_sta_device = esp_radio::wifi::Interface::station();
    let controller = esp_radio::wifi::WifiController::new(
        peripherals.WIFI,
        ControllerConfig::default().with_initial_config(station_config),
    )
    .unwrap();

    let sta_config = embassy_net::Config::dhcpv4(Default::default());

    let rng = Rng::new();
    let seed = (rng.random() as u64) << 32 | rng.random() as u64;

    let (sta_stack, sta_runner) = embassy_net::new(
        wifi_sta_device,
        sta_config,
        mk_static!(StackResources<4>, StackResources::<4>::new()),
        seed,
    );

    spawner.spawn(connection(controller).unwrap());
    spawner.spawn(net_task(sta_runner).unwrap());

    loop {
        if let Some(cfg) = sta_stack.config_v4() {
            println!("Got IP: {}", cfg.address.address());
            break;
        }
        println!("Waiting for IP...");
        Timer::after(Duration::from_millis(500)).await;
    }

    let rx = mk_static!([u8; 4096], [0u8; 4096]);
    let tx = mk_static!([u8; 4096], [0u8; 4096]);
    let ch = TcpChannel::new(sta_stack, DAP_PORT, rx, tx);

    spawner.spawn(dap_tcp_task(ch).unwrap());

    println!("DAP server listening on port {}", DAP_PORT);
    loop {
        Timer::after(Duration::from_secs(3600)).await;
    }
}

#[embassy_executor::task]
async fn dap_tcp_task(mut ch: TcpChannel<'static>) {
    host::run(&mut ch).await;
}

#[embassy_executor::task]
async fn connection(mut controller: WifiController<'static>) {
    println!("start connection task");

    loop {
        match controller.connect_async().await {
            Ok(_) => {
                // wait until we're no longer connected
                loop {
                    let info = embassy_futures::select::select(
                        controller.wait_for_disconnect_async(),
                        controller.wait_for_access_point_connected_event_async(),
                    )
                    .await;

                    match info {
                        Either::First(station_disconnected) => {
                            if let Ok(station_disconnected) = station_disconnected {
                                println!("Station disconnected: {:?}", station_disconnected);
                                break;
                            }
                        }
                        Either::Second(event) => {
                            if let Ok(event) = event {
                                match event {
                                    esp_radio::wifi::ap::EventInfo::Connected(info) => {
                                        println!("Station connected: {:?}", info);
                                    }
                                    esp_radio::wifi::ap::EventInfo::Disconnected(info) => {
                                        println!("Station disconnected: {:?}", info);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                println!("Failed to connect to wifi: {e:?}");
                Timer::after(Duration::from_millis(5000)).await
            }
        }
    }
}

#[embassy_executor::task(pool_size = 2)]
async fn net_task(mut runner: Runner<'static, Interface>) {
    runner.run().await
}
