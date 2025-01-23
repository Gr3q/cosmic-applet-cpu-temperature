// Mandatory COSMIC imports
use cosmic::app::Core;
use cosmic::applet::cosmic_panel_config::PanelAnchor;
use cosmic::cosmic_config::CosmicConfigEntry;
use cosmic::iced::futures::SinkExt;
use cosmic::iced::Rectangle;
use cosmic::iced::{
    platform_specific::shell::commands::popup::{destroy_popup, get_popup},
    widget::{column, horizontal_space, row, vertical_space},
    window::Id,
    Alignment, Length, Subscription, Task,
};
use cosmic::iced_futures::stream;
use cosmic::iced_runtime::core::window;
use cosmic::widget::rectangle_tracker::{rectangle_tracker_subscription, RectangleUpdate};
use cosmic::Element;
use once_cell::sync::Lazy;

// Widgets we're going to use
use cosmic::widget::Id as WidgetID;
use cosmic::widget::{
    autosize, button, container, settings, text_input, toggler, RectangleTracker,
};
use tokio::{sync::watch, time};

use crate::config::CPUTempAppletConfig;
use crate::sysinfo_utils::get_temp;

// Every COSMIC Application and Applet MUST have an ID
const ID: &str = "com.gr3q.CosmicExtAppletCPUTemperature";

static AUTOSIZE_MAIN_ID: Lazy<WidgetID> = Lazy::new(|| WidgetID::new("autosize-main"));

/*
*  Every COSMIC model must be a struct data type.
*  Mandatory fields for a COSMIC Applet are core and popup.
*  Core is the core settings that allow it to interact with COSMIC
*  and popup, as you'll see later, is the field that allows us to open
*  and close the applet.
*
*  Next we have our custom field that we will manipulate the value of based
*  on the message we send.
*/
#[derive(Default)]
pub struct Window {
    core: Core,
    popup: Option<Id>,
    rectangle_tracker: Option<RectangleTracker<u32>>,
    rectangle: Rectangle,
    temp: Option<f32>,
    refresh_period: watch::Sender<u64>,
    period_string: String,
    config: CPUTempAppletConfig,
}

#[derive(Clone, Debug)]
pub enum Message {
    TogglePopup,      // Mandatory for open and close the applet
    PopupClosed(Id),  // Mandatory for the applet to know if it's been closed
    Fahrenheit(bool), // Our custom message to update the isEnabled field on the model
    Rectangle(RectangleUpdate<u32>),
    PeriodString(String),
    Tick,
    ConfigChanged(CPUTempAppletConfig),
}

fn convert_to_fahrenheit(celsius: f32) -> f32 {
    return (celsius * 1.8) + 32.0;
}

impl cosmic::Application for Window {
    /*
     *  Executors are a mandatory thing for both COSMIC Applications and Applets.
     *  They're basically what allows for multi-threaded async operations for things that
     *  may take too long and block the thread the GUI is running on. This is also where
     *  Tasks take place.
     */
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = (); // Honestly not sure what these are for.
    type Message = Message; // These are setting the application messages to our Message enum
    const APP_ID: &'static str = ID; // This is where we set our const above to the actual ID

    // Setup the immutable core functionality.
    fn core(&self) -> &Core {
        &self.core
    }

    // Set up the mutable core functionality.
    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn style(&self) -> Option<cosmic::iced_runtime::Appearance> {
        Some(cosmic::applet::style())
    }

    // Initialize the applet
    /*
     *  The parameters are the Core and flags (again not sure what to do with these).
     *  The function returns our model struct initialized and an Option<Task>, in this case
     *  there is no command so it returns a None value with the type of Task in its place.
     */
    fn init(core: Core, _flags: Self::Flags) -> (Self, Task<cosmic::app::Message<Self::Message>>) {
        let (period, _) = watch::channel(1000);

        let window = Window {
            core, // Set the incoming core
            rectangle_tracker: None,
            rectangle: Rectangle::default(),
            refresh_period: period,
            period_string: "1000".to_string(),
            temp: get_temp(),
            config: CPUTempAppletConfig::default(),
            ..Default::default() // Set everything else to the default values
        };

        (window, Task::none())
    }

    // Create what happens when the applet is closed
    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        // Pass the PopupClosed message to the update function
        Some(Message::PopupClosed(id))
    }

    fn subscription(&self) -> Subscription<Message> {
        fn time_subscription(mut period_watcher: watch::Receiver<u64>) -> Subscription<Message> {
            Subscription::run_with_id(
                "time-sub",
                stream::channel(1, |mut output| async move {
                    // Mark this receiver's state as changed so that it always receives an initial
                    // update during the loop below
                    // This allows us to avoid duplicating code from the loop
                    period_watcher.mark_changed();
                    let mut timer = time::interval(time::Duration::from_millis(1000));
                    timer.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

                    loop {
                        tokio::select! {
                            _ = timer.tick() => {
                                #[cfg(debug_assertions)]
                                if let Err(err) = output.send(Message::Tick).await {
                                    tracing::error!(?err, "Failed sending tick request to applet");
                                }
                                #[cfg(not(debug_assertions))]
                                let _ = output.send(Message::Tick).await;
                            },
                            // Update timer if the user changes the refresh period
                            Ok(()) = period_watcher.changed() => {
                                let milliseconds = *period_watcher.borrow();
                                let time_ms = time::Duration::from_millis(milliseconds);
                                let start = time::Instant::now() + time_ms;
                                timer = time::interval_at(start, time_ms);

                                timer.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
                            }
                        }
                    }
                }),
            )
        }

        let period_rx = self.refresh_period.subscribe();

        Subscription::batch(vec![
            rectangle_tracker_subscription(0).map(|e| Message::Rectangle(e.1)),
            time_subscription(period_rx),
            self.core.watch_config(Self::APP_ID).map(|u| {
                for err in u.errors {
                    tracing::error!(?err, "Error watching config");
                }
                Message::ConfigChanged(u.config)
            }),
        ])
    }

    // Here is the update function, it's the one that handles all of the messages that
    // are passed within the applet.
    fn update(&mut self, message: Self::Message) -> Task<cosmic::app::Message<Self::Message>> {
        // match on what message was sent
        match message {
            // Handle the TogglePopup message
            Message::TogglePopup => {
                // Close the popup
                if let Some(popup_id) = self.popup.take() {
                    return destroy_popup(popup_id);
                } else if let Some(main_window_id) = self.core.main_window_id() {
                    // Create and "open" the popup
                    let new_id = Id::unique();
                    self.popup.replace(new_id);

                    let mut popup_settings = self.core.applet.get_popup_settings(
                        main_window_id,
                        new_id,
                        None,
                        None,
                        None,
                    );

                    let Rectangle {
                        x,
                        y,
                        width,
                        height,
                    } = self.rectangle;

                    popup_settings.positioner.anchor_rect = Rectangle::<i32> {
                        x: x.max(1.) as i32,
                        y: y.max(1.) as i32,
                        width: width.max(1.) as i32,
                        height: height.max(1.) as i32,
                    };

                    popup_settings.positioner.size = Some((300, 500));

                    return get_popup(popup_settings);
                }
            }
            // Unset the popup field after it's been closed
            Message::PopupClosed(popup_id) => {
                if self.popup.as_ref() == Some(&popup_id) {
                    self.popup = None;
                }
            }
            Message::Fahrenheit(fahrenheit) => {
                self.config.fahrenheit = fahrenheit;
                if let Ok(helper) =
                    cosmic::cosmic_config::Config::new(Self::APP_ID, CPUTempAppletConfig::VERSION)
                {
                    if let Err(err) = self.config.write_entry(&helper) {
                        tracing::error!(?err, "Error writing config");
                    }
                }
            }
            Message::Rectangle(u) => match u {
                RectangleUpdate::Rectangle(r) => {
                    self.rectangle = r.1;
                }
                RectangleUpdate::Init(tracker) => {
                    self.rectangle_tracker = Some(tracker);
                }
            },
            Message::Tick => {
                self.temp = get_temp();
            }
            Message::PeriodString(input) => {
                match input.parse::<u64>() {
                    Ok(valid_int) => {
                        if valid_int >= 500 {
                            self.config.refresh_period_milliseconds = valid_int.clone();
                            if let Ok(helper) = cosmic::cosmic_config::Config::new(
                                Self::APP_ID,
                                CPUTempAppletConfig::VERSION,
                            ) {
                                if let Err(err) = self.config.write_entry(&helper) {
                                    tracing::error!(?err, "Error writing config");
                                }
                            }
                        } else {
                            // TODO: Error handling
                        }
                    }
                    Err(_) => {
                        // TODO parse handling
                    }
                }

                self.period_string = input;
            }
            Message::ConfigChanged(c) => {
                // Don't interrupt the tick subscription unless necessary
                self.refresh_period
                    .send_if_modified(|refresh_period_milliseconds| {
                        if *refresh_period_milliseconds == c.refresh_period_milliseconds {
                            false
                        } else {
                            *refresh_period_milliseconds = c.refresh_period_milliseconds;
                            self.period_string = c.refresh_period_milliseconds.to_string();
                            true
                        }
                    });
                self.config = c;
            }
        }

        return Task::none(); // Again not doing anything that requires multi-threading here.
    }

    /*
     *  For an applet, the view function describes what an applet looks like. There's a
     *  secondary view function (view_window) that shows the widgets in the popup when it's
     *  opened.
     */
    fn view(&self) -> Element<Self::Message> {
        let horizontal = matches!(
            self.core.applet.anchor,
            PanelAnchor::Top | PanelAnchor::Bottom
        );

        let mut temp: String = "--".to_string();
        if let Some(temp_value) = self.temp {
            if self.config.fahrenheit {
                temp = format!("{:.0}", convert_to_fahrenheit(temp_value));
            } else {
                temp = format!("{:.0}", temp_value);
            }
            temp.push_str("°");
        }

        let button = button::custom(if horizontal {
            Element::from(
                row!(
                    self.core.applet.text(temp),
                    container(vertical_space().height(Length::Fixed(
                        (self.core.applet.suggested_size(true).1
                            + 2 * self.core.applet.suggested_padding(true))
                            as f32
                    )))
                )
                .align_y(Alignment::Center),
            )
        } else {
            Element::from(
                column!(
                    self.core.applet.text(temp),
                    container(horizontal_space().width(Length::Fixed(
                        (self.core.applet.suggested_size(true).0
                            + 2 * self.core.applet.suggested_padding(true))
                            as f32
                    )))
                )
                .align_x(Alignment::Center),
            )
        })
        .padding(if horizontal {
            [0, self.core.applet.suggested_padding(true)]
        } else {
            [self.core.applet.suggested_padding(true), 0]
        })
        .on_press_down(Message::TogglePopup)
        .class(cosmic::theme::Button::AppletIcon);

        autosize::autosize(
            if let Some(tracker) = self.rectangle_tracker.as_ref() {
                Element::from(tracker.container(0, button).ignore_bounds(true))
            } else {
                button.into()
            },
            AUTOSIZE_MAIN_ID.clone(),
        )
        .into()
    }

    // The actual GUI window for the applet. It's a popup.
    fn view_window(&self, _id: Id) -> Element<Self::Message> {
        // A text box to show if we've enabled or disabled anything in the model
        let content_list = column![
            settings::item(
                "Fahrenheit",
                toggler(self.config.fahrenheit).on_toggle(Message::Fahrenheit),
            ),
            settings::item(
                "Refresh Interval (ms)",
                text_input("1000", self.period_string.clone()).on_input(Message::PeriodString),
            )
        ]
        .padding(self.core.applet.suggested_padding(true))
        .spacing(8);

        // Set the widget content list as the popup_container for the applet
        self.core
            .applet
            .popup_container(container(content_list))
            .into()
    }
}
