use core::convert::TryInto;

use embedded_svc::{
    http::{client::Client as HttpClient, Method},
    io::Write,
    utils::io,
    wifi::{AuthMethod, ClientConfiguration, Configuration, Wifi},
};

//Wifi modules
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::http::client::EspHttpConnection;
use esp_idf_svc::log::EspLogger;
use esp_idf_svc::wifi::{BlockingWifi, EspWifi};
use esp_idf_svc::{eventloop::EspSystemEventLoop, nvs::EspDefaultNvsPartition};

//Time modules
use chrono::{DateTime, TimeZone, Utc};
use esp_idf_svc::sntp;
use esp_idf_svc::sntp::SyncStatus;

use esp_idf_hal::gpio::{Gpio32, Gpio33, Gpio4, Gpio5};
use esp_idf_hal::i2c::{I2cConfig, I2cDriver, I2C0};
use esp_idf_hal::units::FromValueType;

use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyleBuilder},
    pixelcolor::BinaryColor,
    prelude::*,
    text::{Baseline, Text},
};
use ssd1306::{prelude::*, I2CDisplayInterface, Ssd1306};

use log::{error, info};

const SSID: &str = "";
const PASSWORD: &str = "";

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();

    EspLogger::initialize_default();

    //Time before sync
    // info!(
    //     "Time before sync is {:#?}!",
    //     esp_idf_svc::systime::EspSystemTime.now()
    // );

    // Setting up peripherals

    let peripherals = Peripherals::take()?;
    let sys_loop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    let sda = peripherals.pins.gpio32;
    let scl = peripherals.pins.gpio33;
    let i2c = peripherals.i2c0;

    let config = I2cConfig::new().baudrate(100.kHz().into());
    let i2c = I2cDriver::new(i2c, sda, scl, &config)?;
    let interface = I2CDisplayInterface::new(i2c);

    let mut display = Ssd1306::new(interface, DisplaySize128x32, DisplayRotation::Rotate0)
        .into_buffered_graphics_mode();
    display
        .init()
        .map_err(|e| anyhow::anyhow!("Init error: {:?}", e))?;

    let text_style = MonoTextStyleBuilder::new()
        .font(&FONT_6X10)
        .text_color(BinaryColor::On)
        .build();

    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(peripherals.modem, sys_loop.clone(), Some(nvs))?,
        sys_loop,
    )?;

    connect_wifi(&mut wifi)?;

    // Create HTTP(S) client
    let mut client = HttpClient::wrap(EspHttpConnection::new(&Default::default())?);

    //Time syncr
    let sntp = sntp::EspSntp::new_default()?;
    info!("SNTP initialized, waiting for status!");

    let resp = get_request(&mut client).unwrap();
    while sntp.get_sync_status() != SyncStatus::Completed {}
    info!("SNTP status received!");

    loop {
        let ti = esp_idf_svc::systime::EspSystemTime.now().as_secs();
        let human_readable: DateTime<Utc> = Utc.timestamp_opt(ti as i64, 0).unwrap();
        info!(
            "Time after sync {:#?}, {} !",
            ti,
            human_readable.to_string()
        );

        // GET
        // let resp_weather = get_request(&mut client)?;
        Text::with_baseline(
            // &format!("Moscow, Russia:{}", resp_weather),
            &format!("{}\nMSC,RU:{}", human_readable, resp),
            Point::new(0, 0),
            text_style,
            Baseline::Top,
        )
        .draw(&mut display)
        .map_err(|e| anyhow::anyhow!("Txt2 erroconfigr: {:?}", e))?;
        display
            .flush()
            .map_err(|e| anyhow::anyhow!("Flush error: {:?}", e))?;

        std::thread::sleep(std::time::Duration::from_secs(1));
        display.clear(BinaryColor::Off).unwrap();
    }
}

/// Sending an HTTP GET request.
fn get_request(client: &mut HttpClient<EspHttpConnection>) -> anyhow::Result<(String)> {
    // Gotovim zagolovok? no ne obyazatelno
    let headers = [("accept", "text/plain")];
    // let url = "http://ifconfig.net/";
    let url = "https://wttr.in/?format=1"; //
                                           // let url = "https://www.weatherapi.com/weather/q/moscow-2145091";
                                           //https://www.weatherapi.com/weather/q/moscow-2145091

    // Sending request
    //
    let request = client.request(Method::Get, url, &headers)?;
    info!("-> GET {}", url);
    let mut response = request.submit()?;
    // info!("-> response {:?}", response);
    // Process response
    let status = response.status();
    info!("<- {}", status);
    let mut buf = [0u8; 1024];
    let bytes_read = io::try_read_full(&mut response, &mut buf).map_err(|e| e.0)?;
    info!("Read {} bytes", bytes_read);
    let resp = match std::str::from_utf8(&buf[0..bytes_read]) {
        // "🌨  -3°C\n"
        Ok(body_string) => {
            info!(
                "Response body (truncated to {} bytes): {:?}",
                buf.len(),
                body_string
            );
            info!("{:?}", &buf[0..6]);
            body_string.replace("°", "`")
        }
        Err(e) => {
            error!("Error decoding response body: {}", e);
            "error".to_string()
        }
    };

    // while response.read(&mut buf)? > 0 {}

    Ok(String::from(resp))
}

//Wifi connection
fn connect_wifi(wifi: &mut BlockingWifi<EspWifi<'static>>) -> anyhow::Result<()> {
    let wifi_configuration: Configuration = Configuration::Client(ClientConfiguration {
        ssid: SSID.try_into().unwrap(),
        bssid: None,
        auth_method: AuthMethod::WPA2Personal,
        password: PASSWORD.try_into().unwrap(),
        channel: None,
    });

    wifi.set_configuration(&wifi_configuration)?;

    wifi.start()?;
    info!("Wifi started");

    wifi.connect()?;
    info!("Wifi connected");

    wifi.wait_netif_up()?;
    info!("Wifi netif up");

    Ok(())
}
