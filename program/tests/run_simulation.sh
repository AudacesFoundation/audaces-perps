#!/bin/bash
rm -f tests/integration_tests_output/log/output.log
RUST_BACKTRACE=1 cargo test --test integration_tests  -- --nocapture > /dev/null