use std::{
    error::Error,
    fmt::Debug,
    fs::File,
    io::{Read, Write},
    path::Path,
    thread::{self},
};

use async_std::task::{self};
use fltk::{
    app::{self, Receiver, Sender},
    browser::{Browser, BrowserType},
    button::Button,
    enums::{Align, CallbackTrigger, Color, FrameType, LabelType},
    frame::Frame,
    input::Input,
    prelude::{BrowserExt, GroupExt, InputExt, WidgetBase, WidgetExt},
    text::SimpleTerminal,
    window::{DoubleWindow, Window},
};
use json::{JsonError, JsonValue};
use radiobrowser::{ApiStation, RadioBrowserAPI};
use vlc::{Instance, Media, MediaPlayer};

#[derive(Debug, Clone, Copy)]
pub enum Message {
    FetchStations,
    StationsFetchedSuccess,
    FilterStations,
    PlayRequest,
    PauseRequest,
}

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let app = app::App::default();
    let mut win = Window::default().with_size(640, 480);
    win.make_resizable(true);
    let mut frame = Frame::default()
        .with_size(win.width(), win.height())
        .center_of_parent();
    frame.set_frame(FrameType::FlatBox);
    frame.set_color(Color::Black);

    let (tx_message, rx_message) = app::channel::<Message>();

    let mut browser = build_browser(&win);
    browser.set_type(BrowserType::Hold);
    browser.add("no stations to display");
    browser.set_selection_color(Color::Magenta);
    browser.set_label_type(LabelType::Shadow);
    browser.set_label_color(Color::Black);

    let (mut search_input, mut search_button) = build_search(&win);

    let mut status = SimpleTerminal::default()
        .with_align(Align::Right)
        .with_size(win.width() - 40, 40)
        .below_of(&browser, 0);
    status.set_pos(40, status.y());

    let mut play_button = Button::new(win.width() - 40, 0, 40, 40, ">").below_of(&browser, 0);
    let tx_message_clone = tx_message.clone();
    play_button.set_callback(move |_| tx_message_clone.send(Message::PauseRequest));

    search_input.emit(tx_message, Message::FilterStations);
    search_button.emit(tx_message, Message::FilterStations);
    browser.emit(tx_message, Message::PlayRequest);

    win.end();
    win.show();

    let all_stations: &mut Option<Vec<ApiStation>> = &mut None;

    while app.wait() {
        if let Some(msg) = rx_message.recv() {
            match msg {
                Message::FetchStations => {
                    spawn_fetch_thread(&browser, &tx_message, all_stations);
                    if all_stations.is_some() {
                        status.set_text(&format!(
                            "Successfully fetched {} stations",
                            &all_stations.clone().unwrap().len(),
                        ));
                    }
                }
                Message::FilterStations => {
                    if all_stations.is_none() {
                        if !is_cache_present() {
                            spawn_fetch_thread(&browser, &tx_message, all_stations);
                            let json_vec = all_stations
                                .unwrap()
                                .into_iter()
                                .map(|e| station_to_json(e).unwrap())
                                .collect::<Vec<_>>();
                            write_data_to_file(serde_json::to_string(&json_vec));
                            fill_station_browser(&browser, all_stations);
                        }
                    }
                    search_input.set_text_color(Color::from_rgba_tuple((255, 255, 255, 50)));
                    status.set_text(&format!("..."));
                    let filtered_stations = filter_stations(
                        &all_stations.clone().unwrap_or(vec![]),
                        &search_input.value(),
                    );
                    status.set_text(&format!("Found: {}", filtered_stations.len()));

                    browser.clear();
                    for station in filtered_stations.as_slice() {
                        browser.add_with_data(&format_station(&station), station.clone())
                    }
                }
                Message::StationsFetchedSuccess => fill_station_browser(&browser, all_stations),
                Message::PlayRequest => {
                    let station = unsafe {
                        browser
                            .data::<ApiStation>(
                                *browser
                                    .selected_items()
                                    .first()
                                    .expect("Couldn't get selected station"),
                            )
                            .expect("")
                    };

                    let status_text = format!("Playing: {}", &station.url_resolved);
                    status.set_text(&status_text);
                    println!("{}", status_text);
                    let instance = Instance::new().expect("Error initializing vlc instance");
                    let media = Media::new_location(&instance, &station.url_resolved)
                        .expect("Error initializing vlc media");
                    let player =
                        MediaPlayer::new(&instance).expect("Error initializing vlc media player");
                    init_player(
                        play_button.clone(),
                        tx_message.clone(),
                        rx_message.clone(),
                        player,
                        media,
                    );
                }
                Message::PauseRequest => {
                    let _ = &status.set_text("Playback stopped.");
                    play_button.set_label(">");
                }
            }
        };
    }

    Ok(())
}

fn station_to_json(station: ApiStation) -> Result<ApiStation, serde_json::Error> {
    let s = serde_json::json!({
       "changeuuid": station.changeuuid,
       "stationuuid": station.stationuuid,
       "serveruuid": station.serveruuid,
       "name": station.name,
       "url": station.url,
       "url_resolved": station.url_resolved,
       "homepage": station.homepage,
       "favicon": station.favicon,
       "tags": station.tags,
       "country": station.country,
       "countrycode": station.countrycode,
       "iso_3166_2": station.iso_3166_2,
       "state": station.state,
       "language": station.language,
       "languagecodes": station.languagecodes,
       "votes": station.votes,
       "lastchangetime_iso8601": station.lastchangetime_iso8601,
       "codec": station.codec,
       "bitrate": station.bitrate,
       "hls": station.hls,
       "lastcheckok": station.lastcheckok,
       "lastchecktime_iso8601": station.lastchecktime_iso8601,
       "lastcheckoktime_iso8601": station.lastcheckoktime_iso8601,
       "lastlocalchecktime_iso8601": station.lastlocalchecktime_iso8601,
       "clicktimestamp_iso8601": station.clicktimestamp_iso8601,
       "lastchecktime_iso8601": station.lastchecktime_iso8601,
       "lastcheckoktime_iso8601": station.lastcheckoktime_iso8601,
       "lastlocalchecktime_iso8601": station.lastlocalchecktime_iso8601,
       "clicktimestamp_iso8601": station.clicktimestamp_iso8601,
       "clickcount": station.clickcount,
       "clicktrend": station.clicktrend ,
       "ssl_error": station.ssl_error,
       "geo_lat": station.geo_lat,
       "geo_long": station.geo_long,
       "has_extended_info": station.has_extended_info,
    });
    let json_value = serde_json::from_value::<ApiStation>(s);

    json_value
}

fn init_player(
    play_button: Button,
    _tx_mediastate: Sender<Message>,
    rx_mediastate: Receiver<Message>,
    player: MediaPlayer,
    media: Media,
) {
    player.set_media(&media);
    player.play().expect("Error playing vlc media");
    let play_button_ref = &mut play_button.clone();
    play_button_ref.set_label("||");
    let _player_thread = thread::spawn(move || loop {
        thread::sleep(std::time::Duration::from_secs(u64::MAX));
        match rx_mediastate.recv() {
            Some(msg) => {
                println!("{:?}", msg)
            }
            None => {
                println!("idk fuggin chickem nuggies")
            }
        }
    });
}

fn is_cache_present() -> bool {
    let path = Path::new("stations.json");
    let _ = match File::open(path) {
        Err(_) => return false,
        Ok(_) => return true,
    };
}

fn write_data_to_file(data: &str) {
    let path = Path::new("stations.json");
    let display = path.display();
    let mut file = match File::create(&path) {
        Err(why) => panic!("couldn't create {}:{}", display, why),
        Ok(file) => file,
    };
    match file.write_all(data.as_bytes()) {
        Err(why) => panic!("couldn't write to {}: {}", display, why),
        Ok(_) => println!("Successfully wrote to: {}", display),
    }
}

fn read_data_from_file_and_parse() -> Result<JsonValue, JsonError> {
    let path = Path::new("stations.json");
    let display = path.display();
    let mut file = match File::open(&path) {
        Err(why) => panic!("couldn't open {}: {}", display, why),
        Ok(file) => file,
    };
    let mut s = String::new();
    match file.read_to_string(&mut s) {
        Err(why) => panic!("couldn't read {}: {}", display, why),
        Ok(_) => print!("{} contains: \n{}", display, s),
    }
    let parsed = json::parse(&s);

    parsed
}

fn spawn_fetch_thread(
    browser: &Browser,
    tx_fetch_signal: &Sender<Message>,
    stations_container: &mut Option<Vec<ApiStation>>,
) {
    task::block_on(async {
        let stations = fetch_stations().await.unwrap_or(vec![]);
        tx_fetch_signal.send(Message::StationsFetchedSuccess);

        let _ = stations_container.insert(stations.clone());
        fill_station_browser(&browser, &mut Some(stations.clone()));
    });
}

fn fill_station_browser(browser: &Browser, stations: &mut Option<Vec<ApiStation>>) {
    if stations.is_some() {
        let mut browser_ref = browser.clone();
        if stations.clone().unwrap().len() < 1 {
            browser_ref.add("Received 0 stations.")
        }
        browser_ref.clear();
        for station in stations.clone().unwrap() {
            browser_ref.add_with_data(&format_station(&station), station.clone());
        }
    }
}

fn build_search(window: &DoubleWindow) -> (Input, Button) {
    let mut input = Input::new(0, 0, window.width() - 100, 40, "");
    input.set_label("Search");
    input.set_trigger(CallbackTrigger::EnterKey);

    let search_button = Button::new(window.width() - 100, 0, 100, 40, "Search");

    (input, search_button)
}

fn build_browser(window: &DoubleWindow) -> Browser {
    let mut browser = Browser::new(0, 40, window.width(), window.height() - 80, "");
    browser.set_has_scrollbar(fltk::browser::BrowserScrollbar::Vertical);
    let num_of_columns = 4;
    let col_width = window.width() / num_of_columns;
    let col_widths = (0..=num_of_columns).map(|_| col_width).collect::<Vec<_>>();
    browser.set_column_widths(&col_widths);
    browser.set_column_char('|');

    browser
}

async fn fetch_stations() -> Result<Vec<ApiStation>, Box<dyn Error>> {
    RadioBrowserAPI::new().await?.get_stations().send().await
}

fn filter_stations(stations: &Vec<ApiStation>, filter: &str) -> Vec<ApiStation> {
    stations
        .clone()
        .into_iter()
        .filter(|v| {
            if filter.len() > 0 {
                v.name.contains(filter) || v.tags.contains(filter)
            } else {
                true
            }
        })
        .collect::<Vec<_>>()
}

fn format_station(station: &ApiStation) -> String {
    format!(
        "{}|{}|{}|{}",
        (if station.name.is_empty() {
            station.url_resolved.to_ascii_lowercase()
        } else {
            station.name.to_ascii_lowercase()
        })
        .trim_end()
        .trim_start(),
        station.state,
        station.country,
        station.tags
    )
}
