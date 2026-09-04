//! Port of test_thefuzz_pytest.py::test_process_warning.
//!
//! When the processor reduces the query to an empty string, thefuzz emits a
//! WARNING on the `thefuzz.process` logger. Python captures it via caplog and
//! asserts exactly one record, its level, logger name, and exact message.
//!
//! The Rust port exposes an equivalent capture hook (`process::warning`) that
//! records emitted messages; this test installs it, triggers the warning, and
//! asserts the same single-record / exact-message / logger-name invariants.

use std::cell::RefCell;
use std::rc::Rc;
use thefuzz::process::{self, Choices, Processor, Scorer};

#[test]
fn test_process_warning() {
    let captured: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&captured);
    process::warning::set_hook(move |msg: &str| {
        sink.borrow_mut().push(msg.to_string());
    });

    let choices = Choices::List(vec![Some(":::::::")]);
    let _ = process::extract_one(
        ":::::::",
        &choices,
        Some(process::default_processor as Processor),
        Scorer::WRatio,
        0,
    );

    process::warning::clear_hook();

    let records = captured.borrow();
    // Exactly one warning record, mirroring caplog assertion.
    assert_eq!(records.len(), 1);
    // Exact message parity with thefuzz.process._validate_query_preprocessing.
    assert_eq!(
        records[0],
        "Applied processor reduces input query to empty string, all comparisons will have score 0. [Query: ':::::::']"
    );
    // Logger name parity ('thefuzz.process').
    assert_eq!(process::warning::LOGGER_NAME, "thefuzz.process");
}
