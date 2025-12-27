use super::Emitter;

struct TerminalEmitter {}

impl Emitter for TerminalEmitter {
    fn emit<'a>(&mut self, events: impl Iterator<Item = pulldown_cmark::Event<'a>>) -> String {
        unimplemented!()
    }
}
