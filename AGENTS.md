The monorepo is used for rapid prototyping of a few interrelated projects. Directories contain project implementations, with suffixes hinting at the language (e.g., `-rs` for Rust, `-py` for Python).

## Code Guidelines

Keep the code lean and clean. Avoid too much error handling code, in many cases, it is ok to just wrap the error as it is and pass it further. Make the code self-documenting wherever possible, without unnecessary verbose comments. The only comments that should be introduced are ones that explain **why** and **how** something is done, where it is not obvious from the context.

Add tests to verify the correctness of the implementation. As it is a prototype, minimize the amount of code required to do that. The tests don't need to be fine-grained unit tests, it is ok to test a few things together.
