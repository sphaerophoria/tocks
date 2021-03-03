# Tocks

A Tox client with no good reason to exist. Please check out [qtox](https://github.com/qTox/qTox) for a fully featured and maintained project.

# Goals

* First and foremost the author's enjoyment
* Well enough abstracted that UI toolkits can be swapped out in the future
* Stable/well tested/easily testable
* Not necessarily feature packed. Whatever people want to implement can be implemented

# Technical details

* The current plan is to implement a rust based backend and start with a QML based frontend. The project will likely be laid out as a bunch of crates providing subsets of functionality (toxcore bindings, UI, general backend stuff) with a main crate to tie it all together.

# Status

* Not even started
