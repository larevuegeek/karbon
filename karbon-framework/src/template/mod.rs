//! # Template engines
//!
//! Server-side rendering behind a backend-agnostic [`Renderer`] trait, with two
//! interchangeable backends selected by feature flag:
//!
//! - **`templates`** → [`TemplateEngine`] (Tera), with custom filters and
//!   integrated email sending.
//! - **`minijinja`** → [`MinijinjaEngine`] (minijinja).
//!
//! Both use Jinja2/Twig syntax. Application code can depend on `Arc<dyn Renderer>`
//! to stay backend-independent; a future in-house engine will implement the same
//! trait.
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
