use ftui_core::event::Event;
use ftui_render::frame::Frame;
use ftui_runtime::program::{Cmd, Model};
use ftui_runtime::simulator::ProgramSimulator;

struct TestModel {
    executed_after_quit: bool,
}

#[derive(Debug)]
enum TestMsg {
    QuitInBatch,
    SetExecuted,
}

impl From<Event> for TestMsg {
    fn from(_: Event) -> Self {
        TestMsg::QuitInBatch
    }
}

impl Model for TestModel {
    type Message = TestMsg;

    fn update(&mut self, msg: Self::Message) -> Cmd<Self::Message> {
        match msg {
            TestMsg::QuitInBatch => Cmd::Batch(vec![
                Cmd::Quit,
                Cmd::Msg(TestMsg::SetExecuted), // Should NOT be executed
            ]),
            TestMsg::SetExecuted => {
                self.executed_after_quit = true;
                Cmd::None
            }
        }
    }

    fn view(&self, _frame: &mut Frame) {}
}

#[test]
fn batch_stops_after_quit() {
    let mut sim = ProgramSimulator::new(TestModel {
        executed_after_quit: false,
    });
    sim.init();

    sim.send(TestMsg::QuitInBatch);

    // Check if the model state changed after quit
    assert!(
        !sim.model().executed_after_quit,
        "Commands after Quit in Batch should not be executed"
    );
    assert!(!sim.is_running(), "Simulator should have stopped");
}
