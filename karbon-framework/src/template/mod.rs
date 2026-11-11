//! # Template engines
//!
//! Server-side rendering behind a backend-agnostic [`Renderer`] trait.
//!
//! - **`templates`** → [`TemplateEngine`] (Tera). **The supported engine**: custom
//!   filters, email sending, and the one `AppState.templates` holds. Pick this one.
//! - **`minijinja`** → [`MinijinjaEngine`] and **`native-templates`** → [`NativeEngine`]
//!   (in-house, dependency-free) are **experimental**: they implement [`Renderer`], but
//!   their feature set and API may change or go away in a minor release.
//!
//! All use Jinja2/Twig syntax. Application code can depend on `Arc<dyn Renderer>` to stay
//! backend-independent.
//!
//! ```ignore
//! {% extends "layouts/base.html" %}
//! {% block content %}<h1>Hello {{ username }}!</h1>{% endblock %}
//! ```

mod renderer;
pub use renderer::Renderer;

#[cfg(feature = "templates")]
mod engine;
#[cfg(feature = "templates")]
mod filters;
#[cfg(feature = "templates")]
pub use engine::TemplateEngine;

#[cfg(feature = "minijinja")]
mod minijinja_engine;
#[cfg(feature = "minijinja")]
pub use minijinja_engine::MinijinjaEngine;

#[cfg(feature = "native-templates")]
mod native;
#[cfg(feature = "native-templates")]
pub use native::NativeEngine;
