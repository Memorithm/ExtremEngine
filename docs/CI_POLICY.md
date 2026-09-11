# CI and Merge Policy

ExtremEngine changes targeting `main` must be validated on the exact pull-request HEAD before merge.

Required evidence for a merge:

- `Code Quality & Security` completed successfully;
- Ubuntu test matrix on the declared MSRV completed successfully;
- Ubuntu test matrix on stable Rust completed successfully;
- Windows native compile completed successfully;
- macOS native compile completed successfully.

A queued, pending, skipped, cancelled, `action_required`, or failed run is not equivalent to a successful run.

When a pull-request HEAD changes, validation from an older commit must not be used as evidence for the new HEAD. The current HEAD SHA and its workflow run must be checked immediately before merging.

`cargo-deny` warnings that do not fail the command are findings to review, but only the command exit status determines that particular CI step. Failing format, compile, test, clippy, rustdoc, or security steps must be corrected rather than bypassed.

Hardware-dependent GPU presentation is reported separately from compile/test CI. A CI environment without a suitable GPU must not be described as having validated hardware presentation.
