# daw-rt — keeping the audio thread real-time safe

The audio thread has a few milliseconds to produce each block of sound. If it
waits for anything (memory from the system, a lock held by another thread, a
file, the network, a log message) the result is an audible click or dropout.
This crate holds the small tools that keep it honest.

## What belongs here

- **The allocation check.** A wrapper around the memory allocator. Code that
  runs on the audio thread marks itself as real-time; in debug builds, any
  memory allocation or release inside such a region stops the program with a
  message saying so. Release builds compile the check away entirely.
- Later: other lock-free building blocks shared by the engine, the built-in
  instruments and plugin hosting.

## Rules

- Depends on nothing else in the application, so every crate can use it.
- Everything in it must itself be safe to call from the audio thread.
- The check must never allocate, lock or log while reporting a problem: it
  writes a fixed message straight to the error output and aborts.

## Roadmap

Phase 0 (the allocation check, used by the audio spike), phase 1 onwards
(used by the engine on every audio block).
