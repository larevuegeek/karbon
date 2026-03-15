use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

/// Escape a string for safe HTML output inside a LiveComponent's render().
///
/// ```ignore
/// fn render(&self) -> String {
///     format!("<span>{}</span>", html_escape(&self.user_input))
/// }
/// ```
pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Trait for server-rendered live components.
///
/// A LiveComponent holds state, renders HTML, and handles events from the browser.
/// When an event is received via WebSocket, `handle_event` is called, the state
/// is updated, and `render` produces new HTML that is sent back to the client.
///
/// **SECURITY**: The HTML returned by `render()` is sent directly to the browser.
/// Always use `html_escape()` for any user-provided content to prevent XSS.
///
/// ```ignore
/// struct Counter {
///     count: i32,
/// }
///
/// impl LiveComponent for Counter {
///     fn render(&self) -> String {
///         format!(r#"
///             <div>
///                 <span>{}</span>
///                 <button lw-click="increment">+</button>
///                 <button lw-click="decrement">-</button>
///             </div>
///         "#, self.count)
///     }
///
///     fn handle_event<'a>(
///         &'a mut self,
///         event: &'a str,
///         _params: &'a HashMap<String, String>,
///     ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
///         Box::pin(async move {
///             match event {
///                 "increment" => self.count += 1,
///                 "decrement" => self.count -= 1,
///                 _ => {}
///             }
///         })
///     }
/// }
/// ```
pub trait LiveComponent: Send + Sync + 'static {
    /// Render the component to an HTML string
    fn render(&self) -> String;

    /// Handle an event from the browser, mutating state as needed
    fn handle_event<'a>(
        &'a mut self,
        event: &'a str,
        params: &'a HashMap<String, String>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>;

    /// Called once when the component is first mounted (optional)
    fn mount(&mut self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async {})
    }
}
