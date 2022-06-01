## Testing

- [x] Add testing for control.rs
- [x] Add testing for lib.rs
- [x] Add testing for response.rs

## PMD Data

- [x] Switch PmdRead to store a vec of data types since multiple samples can come with one response
- [x] Abstract PmdRead and related structs/enums to their own file
- [x] Figure out how to read control point response
- [x] Fix ControlResponse so it can read and understand data
- [x] Add builtin support for what each value from `PolarSensor.settings()` means

## User API

- [x] Add closing condition function to event handler trait so users can specify condition to stop eventloop without keyboard interrupt or walking far enough away the Bluetooth disconnects
- [x] Add support for multiple resolutions and sample rates for different data types
- [x] Add support for listening for multiple kinds of PMD data (ecg AND acc)
- [x] Add structure to store heart rate data (including BPM and RR interval)
- [x] Add new api to start and stop measurement while event loop is running using async or threads (potential feature)
- [x] Disallow user from starting event loop without subscribing to anything

## Control Point Commands

- [x] Generate requests using `PolarSensor` fields (`resolution`, `data_type`, `sample_rate`) instead of hard-coded requests

## Examples

- [x] Create examples directory and move everything around
- [x] Create example that tries to use multithreading

DONE!!
