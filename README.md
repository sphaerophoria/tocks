# Tocks

A Tox client with no good reason to exist. Please check out [qtox](https://github.com/qTox/qTox) for a fully featured and maintained project.

# Goals

* First and foremost the author's enjoyment
* Well enough abstracted that UI toolkits can be swapped out in the future
* Stable/well tested/easily testable
* Not necessarily feature packed. Whatever people want to implement can be implemented

# Technical details

The project is split into several crates
* toxcore-sys: Bindings to c-toxcore
* toxcore: Rust API for tox functionality
* tocks: Application logic/state
* ui: QML UI around tocks library

Currently toxcore/tocks are tied to tokio. At a high level, tocks runs a future on the tokio runtime that waits for an "event" to happen. This could be anything that triggers us to take an action (e.g. the user sends a mesage, or a friend request is received from toxcore). Once we get an event we handle it and wait for the next one.

At the moment the design is that each subset of functionality will wait for something to happen, and when it does bubble an event up to the top of the tocks app. The tocks main loop will then dispatch the event back down to whatever component needs to handle it. This approach allows us to parallelize all reading of state, but still enforce one mutable writer in the handler portion. Hopefully performance of this pattern will not be so bad that events back up, otherwise we will have to split functionality further so we can handle several types of events at once.

# Status

* Proof of concept QML UI implemented that can login + send/receive messages to friends
