mod html;
mod term;

pub use html::HtmlEmitter;

pub trait Emitter {
    fn emit<'a>(&mut self, events: impl Iterator<Item = pulldown_cmark::Event<'a>>) -> String;
}
