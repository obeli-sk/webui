//! Unified notification system for displaying success, error, and info messages.
//!
//! Usage:
//! ```rust
//! let notifications = use_context::<NotificationContext>().unwrap();
//! notifications.push(Notification::success("Operation completed"));
//! notifications.push(Notification::error("Something went wrong"));
//! ```

use gloo::timers::callback::Timeout;
use std::cell::RefCell;
use std::rc::Rc;
use yew::prelude::*;

/// Default time in milliseconds before a notification auto-dismisses
const AUTO_DISMISS_MS: u32 = 5000;

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

/// Context for managing notifications throughout the application
#[derive(Clone)]
pub struct NotificationContext {
    state: UseStateHandle<Vec<Notification>>,
    next_id: Rc<RefCell<NotificationId>>,
    /// Callback to schedule removal after timeout
    schedule_removal: Callback<NotificationId>,
}

impl PartialEq for NotificationContext {
    fn eq(&self, other: &Self) -> bool {
        // Compare by the current notification list
        *self.state == *other.state
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
        let notification = builder.build(id);

        let mut notifications = (*self.state).clone();
        notifications.push(notification);
        self.state.set(notifications);

        // Schedule auto-removal
        self.schedule_removal.emit(id);
    }

    /// Dismiss a notification by ID (starts fade-out animation)
    pub fn dismiss(&self, id: NotificationId) {
        let mut notifications = (*self.state).clone();
        if let Some(notification) = notifications.iter_mut().find(|n| n.id == id) {
            notification.fading_out = true;
        }
        self.state.set(notifications);
    }

    /// Remove a notification immediately (after fade-out animation completes)
    pub fn remove(&self, id: NotificationId) {
        let notifications: Vec<_> = (*self.state)
            .iter()
            .filter(|n| n.id != id)
            .cloned()
            .collect();
        self.state.set(notifications);
    }
}

/// Properties for the NotificationProvider component
#[derive(Properties, PartialEq)]
pub struct NotificationProviderProps {
    pub children: Children,
}

/// Provider component that wraps the application and provides notification context
#[function_component(NotificationProvider)]
pub fn notification_provider(props: &NotificationProviderProps) -> Html {
    let notifications_state = use_state(Vec::<Notification>::new);
    let next_id = use_mut_ref(|| 0u32);
    let timeouts = use_mut_ref(Vec::<(NotificationId, Timeout)>::new);

    // Create callback for scheduling removal
    let schedule_removal = {
        let notifications_state = notifications_state.clone();
        let timeouts = timeouts.clone();
        Callback::from(move |id: NotificationId| {
            let notifications_state = notifications_state.clone();
            let timeouts_for_store = timeouts.clone();

            // First timeout: start fade-out animation
            let timeout = Timeout::new(AUTO_DISMISS_MS, move || {
                // Mark as fading out
                let mut notifications = (*notifications_state).clone();
                if let Some(notification) = notifications.iter_mut().find(|n| n.id == id) {
                    notification.fading_out = true;
                }
                notifications_state.set(notifications);

                // Second timeout: remove after animation
                let notifications_state = notifications_state.clone();
                let _remove_timeout = Timeout::new(300, move || {
                    let notifications: Vec<_> = (*notifications_state)
                        .iter()
                        .filter(|n| n.id != id)
                        .cloned()
                        .collect();
                    notifications_state.set(notifications);
                });
                // Note: remove_timeout is leaked intentionally - it will fire and be dropped
            });

            timeouts_for_store.borrow_mut().push((id, timeout));
        })
    };

    let context = NotificationContext {
        state: notifications_state.clone(),
        next_id,
        schedule_removal,
    };

    // Create dismiss callback for the toast component
    let on_dismiss = {
        let context = context.clone();
        Callback::from(move |id: NotificationId| {
            context.dismiss(id);
            // Remove after animation
            let context = context.clone();
            let _timeout = Timeout::new(300, move || {
                context.remove(id);
            });
        })
    };

    html! {
        <ContextProvider<NotificationContext> context={context}>
            { props.children.clone() }
            <NotificationToast
                notifications={(*notifications_state).clone()}
                on_dismiss={on_dismiss}
            />
        </ContextProvider<NotificationContext>>
    }
}

/// Properties for the NotificationToast component
#[derive(Properties, PartialEq)]
struct NotificationToastProps {
    notifications: Vec<Notification>,
    on_dismiss: Callback<NotificationId>,
}

/// Component that renders the notification toasts
#[function_component(NotificationToast)]
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

                let class = classes!(
                    "notification-toast",
                    notification.level.css_class(),
                    notification.fading_out.then_some("notification-fading-out")
                );

                html! {
                    <div class={class} key={notification.id}>
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
