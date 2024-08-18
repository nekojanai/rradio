use std::{
    error::Error,
    thread,
    time::{self, Instant},
};

use async_std::task;
use fltk::{
    app::{self, Receiver, Sender},
    browser::{Browser, BrowserType},
    button::Button,
    enums::{CallbackTrigger, Color, FrameType},
    frame::Frame,
    input::Input,
    prelude::{BrowserExt, GroupExt, InputExt, WidgetBase, WidgetExt},
    text::SimpleTerminal,
    window::{DoubleWindow, Window},
};
use radiobrowser::{ApiStation, RadioBrowserAPI};
use vlc::{Instance, Media, MediaPlayer, State};

#[derive(Debug, Clone, Copy)]
pub enum Message {
    FetchStations,
    StationsFetchedSuccess,
    FilterStations,
    PlayRequest,
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
    let (tx_mediastate, rx_mediastate) = app::channel::<State>();
    let fetch_timer = Instant::now();

    let mut browser = build_browser(&win);
    browser.set_type(BrowserType::Hold);
    browser.add("no stations to display");
    browser.set_selection_color(Color::Magenta);

    let (mut search_input, mut search_button) = build_search(&win);

    let mut status = SimpleTerminal::default()
        .with_size(win.width(), 40)
        .below_of(&browser, 0);

    search_input.emit(tx_message, Message::FilterStations);
    search_button.emit(tx_message, Message::FilterStations);
    browser.emit(tx_message, Message::PlayRequest);

    win.end();
    win.show();

    let all_stations: &mut Option<Vec<ApiStation>> = &mut None;

    while app.wait() {
        if all_stations.is_none() {
            spawn_fetch_thread(&tx_message, all_stations);
            fill_station_browser(&browser, all_stations);
        }
        if let Some(msg) = rx_message.recv() {
            match msg {
                Message::FetchStations => {
                    if all_stations.is_some() {
                        status.set_text(&format!(
                            "Successfully fetched {} stations in {}",
                            &all_stations.clone().unwrap().len(),
                            fetch_timer.duration_since(Instant::now()).as_millis()
                        ));
                    }
                    spawn_fetch_thread(&tx_message, all_stations);
                }
                Message::FilterStations => {
                    search_input.set_text_color(Color::from_rgba_tuple((255, 255, 255, 50)));
                    status.set_text(&format!("..."));
                    let filtered_stations =
                        filter_stations(&all_stations.clone().unwrap(), &search_input.value());
                    status.set_text(&format!("Found: {}", filtered_stations.len()));

                    browser.clear();
                    for station in filtered_stations.as_slice() {
                        browser.add_with_data(&format_station(&station), station.clone())
                    }
                }
                Message::StationsFetchedSuccess => {
                    if all_stations.is_some() {
                        status.set_text(&format!(
                            "Successfully fetched {} stations in {}",
                            &all_stations.clone().unwrap().len(),
                            fetch_timer.duration_since(Instant::now()).as_millis()
                        ));
                    }
                }
                Message::PlayRequest => {
                    browser.selected_items().iter().for_each(|&line| unsafe {
                        let station = browser
                            .data::<ApiStation>(line)
                            .expect("Error selecting station");
                        status.set_text(&format!("Playing: {}", &station.url_resolved));
                        init_player(
                            station.url_resolved,
                            tx_mediastate.clone(),
                            rx_mediastate.clone(),
                        );
                    });
                }
            }
        };
    }

    Ok(())
}

fn init_player(url: String, _tx_mediastate: Sender<State>, _rx_mediastate: Receiver<State>) {
    print!("{}", url);
    let _player_thread = thread::spawn(move || {
        let instance = Instance::new().expect("Error initializing vlc instance");
        let media = Media::new_location(&instance, &url).expect("Error initializing vlc media");
        let player = MediaPlayer::new(&instance).expect("Error initializing vlc media player");
        player.set_media(&media);
        player.play().expect("Error playing vlc media");

        loop {
            thread::sleep(time::Duration::from_millis(250));
            if !player.is_playing() {
                break;
            }
        }
    });
}

fn spawn_fetch_thread(
    tx_fetch_signal: &Sender<Message>,
    stations_container: &mut Option<Vec<ApiStation>>,
) {
    task::block_on(async {
        println!("start");
        tx_fetch_signal.send(Message::StationsFetchedSuccess);
        let _ = stations_container.insert(filter_unusables(&fetch_stations().await));
    });
}

fn fill_station_browser(browser: &Browser, stations: &mut Option<Vec<ApiStation>>) {
    if stations.is_some() {
        for station in stations.clone().unwrap() {
            let mut browser_ref = browser.clone();
            browser_ref.clear();
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

async fn fetch_stations() -> Vec<ApiStation> {
    let stations = RadioBrowserAPI::new()
        .await
        .expect("Error initializing RadionBrowserAPI")
        .get_stations()
        .send()
        .await
        .expect("Error while fetching stations");

    stations
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

fn filter_unusables(stations: &Vec<ApiStation>) -> Vec<ApiStation> {
    stations
        .clone()
        .into_iter()
        .filter(|v| !v.url_resolved.is_empty())
        .collect::<Vec<_>>()
}

fn format_station(station: &ApiStation) -> String {
    format!(
        "{}|{}|{}|{}",
        if station.name.is_empty() {
            station.url_resolved.to_ascii_lowercase()
        } else {
            station.name.to_ascii_lowercase()
        },
        station.state,
        station.country,
        station.tags
    )
}
