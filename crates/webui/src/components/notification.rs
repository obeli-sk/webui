//! Unified notification system for displaying success, error, and info messages.
//!
//! Usage:
//! ```ignore
//! let notifications = use_context::<NotificationContext>().unwrap();
//! notifications.push(Notification::success("Operation completed"));
//! notifications.push(Notification::error("Something went wrong"));
//! ```

use gloo::timers::callback::Timeout;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use yew::prelude::*;

/// Default time in milliseconds before a notification auto-dismisses
const AUTO_DISMISS_MS: u32 = 5000;
/// Must match the fade-out transition in `_notifications.scss`
const FADE_OUT_MS: u32 = 300;

/// Unique identifier for notifications
type NotificationId = u32;

/// Notification severity level
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NotificationLevel {
    Info,
    Success,
    Error,
}

impl NotificationLevel {
    fn css_class(&self) -> &'static str {
        match self {
            NotificationLevel::Info => "notification-info",
            NotificationLevel::Success => "notification-success",
            NotificationLevel::Error => "notification-error",
        }
    }

    fn icon(&self) -> &'static str {
        match self {
            NotificationLevel::Info => "ℹ",
            NotificationLevel::Success => "✓",
            NotificationLevel::Error => "✕",
        }
    }
}

/// A notification to display to the user
#[derive(Clone, PartialEq)]
pub struct Notification {
    id: NotificationId,
    pub level: NotificationLevel,
    pub message: String,
    /// Whether the notification is fading out (for animation)
    fading_out: bool,
}

impl Notification {
    fn new(id: NotificationId, level: NotificationLevel, message: impl Into<String>) -> Self {
        Self {
            id,
            level,
            message: message.into(),
            fading_out: false,
        }
    }

    pub fn info(message: impl Into<String>) -> NotificationBuilder {
        NotificationBuilder {
            level: NotificationLevel::Info,
            message: message.into(),
        }
    }

    pub fn success(message: impl Into<String>) -> NotificationBuilder {
        NotificationBuilder {
            level: NotificationLevel::Success,
            message: message.into(),
        }
    }

    pub fn error(message: impl Into<String>) -> NotificationBuilder {
        NotificationBuilder {
            level: NotificationLevel::Error,
            message: message.into(),
        }
    }
}

/// Builder for creating notifications (allows future expansion with options)
pub struct NotificationBuilder {
    level: NotificationLevel,
    message: String,
}

impl NotificationBuilder {
    fn build(self, id: NotificationId) -> Notification {
        Notification::new(id, self.level, self.message)
    }
}

enum NotificationAction {
    Push(Notification),
    FadeOut(NotificationId),
    Remove(NotificationId),
}

#[derive(Default, PartialEq)]
struct Notifications(Vec<Notification>);

impl Reducible for Notifications {
    type Action = NotificationAction;

    fn reduce(self: Rc<Self>, action: Self::Action) -> Rc<Self> {
        let mut list = self.0.clone();
        match action {
            NotificationAction::Push(notification) => list.push(notification),
            NotificationAction::FadeOut(id) => {
                if let Some(notification) = list.iter_mut().find(|n| n.id == id) {
                    notification.fading_out = true;
                }
            }
            NotificationAction::Remove(id) => list.retain(|n| n.id != id),
        }
        Rc::new(Notifications(list))
    }
}

/// Pending auto-dismiss timers, dropping a `Timeout` cancels it.
#[derive(Clone)]
struct DismissTimers {
    state: UseReducerHandle<Notifications>,
    timers: Rc<RefCell<HashMap<NotificationId, Timeout>>>,
}

impl DismissTimers {
    fn schedule(&self, id: NotificationId) {
        let this = self.clone();
        let timeout = Timeout::new(AUTO_DISMISS_MS, move || this.dismiss(id));
        self.timers.borrow_mut().insert(id, timeout);
    }

    fn pause(&self, id: NotificationId) {
        self.timers.borrow_mut().remove(&id);
    }

    /// Start the fade-out animation, then remove the notification.
    fn dismiss(&self, id: NotificationId) {
        self.state.dispatch(NotificationAction::FadeOut(id));
        let this = self.clone();
        Timeout::new(FADE_OUT_MS, move || {
            this.state.dispatch(NotificationAction::Remove(id));
            this.timers.borrow_mut().remove(&id);
        })
        .forget();
    }
}

/// Context for managing notifications throughout the application
#[derive(Clone)]
pub struct NotificationContext {
    timers: DismissTimers,
    next_id: Rc<RefCell<NotificationId>>,
}

impl PartialEq for NotificationContext {
    fn eq(&self, other: &Self) -> bool {
        self.timers.state == other.timers.state
    }
}

impl NotificationContext {
    /// Push a new notification
    pub fn push(&self, builder: NotificationBuilder) {
        let id = {
            let mut next_id = self.next_id.borrow_mut();
            let id = *next_id;
            *next_id = next_id.wrapping_add(1);
            id
        };
        if builder.level == NotificationLevel::Error {
            log::error!("Notification: {}", builder.message);
        }
        self.timers
            .state
            .dispatch(NotificationAction::Push(builder.build(id)));
        self.timers.schedule(id);
    }
}

/// Properties for the NotificationProvider component
#[derive(Properties, PartialEq)]
pub struct NotificationProviderProps {
    pub children: Children,
}

/// Provider component that wraps the application and provides notification context
#[component(NotificationProvider)]
pub fn notification_provider(props: &NotificationProviderProps) -> Html {
    let state = use_reducer(Notifications::default);
    let next_id = use_mut_ref(|| 0u32);
    let timer_map = use_mut_ref(HashMap::new);
    let timers = DismissTimers {
        state: state.clone(),
        timers: timer_map,
    };

    let on_dismiss = {
        let timers = timers.clone();
        Callback::from(move |id| {
            timers.pause(id);
            timers.dismiss(id);
        })
    };
    let on_hover = {
        let timers = timers.clone();
        Callback::from(move |id| timers.pause(id))
    };
    let on_unhover = {
        let timers = timers.clone();
        Callback::from(move |id| timers.schedule(id))
    };

    let context = NotificationContext { timers, next_id };

    html! {
        <ContextProvider<NotificationContext> context={context}>
            { props.children.clone() }
            <NotificationToast
                notifications={state.0.clone()}
                {on_dismiss}
                {on_hover}
                {on_unhover}
            />
        </ContextProvider<NotificationContext>>
    }
}

/// Properties for the NotificationToast component
#[derive(Properties, PartialEq)]
struct NotificationToastProps {
    notifications: Vec<Notification>,
    on_dismiss: Callback<NotificationId>,
    on_hover: Callback<NotificationId>,
    on_unhover: Callback<NotificationId>,
}

/// Component that renders the notification toasts
#[component(NotificationToast)]
fn notification_toast(props: &NotificationToastProps) -> Html {
    if props.notifications.is_empty() {
        return html! {};
    }

    html! {
        <div class="notification-container">
            { for props.notifications.iter().map(|notification| {
                let id = notification.id;
                let on_dismiss = props.on_dismiss.clone();
                let onclick = Callback::from(move |_| on_dismiss.emit(id));
                let onmouseenter = props.on_hover.reform(move |_| id);
                let fading_out = notification.fading_out;
                let on_unhover = props.on_unhover.clone();
                let onmouseleave = Callback::from(move |_| {
                    if !fading_out {
                        on_unhover.emit(id);
                    }
                });

                let class = classes!(
                    "notification-toast",
                    notification.level.css_class(),
                    notification.fading_out.then_some("notification-fading-out")
                );

                html! {
                    <div class={class} key={notification.id} {onmouseenter} {onmouseleave}>
                        <span class="notification-icon">{ notification.level.icon() }</span>
                        <span class="notification-message">{ &notification.message }</span>
                        <button
                            class="notification-dismiss"
                            onclick={onclick}
                            aria-label="Dismiss notification"
                        >
                            {"×"}
                        </button>
                    </div>
                }
            })}
        </div>
    }
}
