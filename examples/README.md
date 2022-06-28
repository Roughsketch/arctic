# Examples

Each example can be run by running from this directory using `cargo run -p <PROJECT-NAME>`.

## `hr-data`

This example just subscribes to HR data and prints its output to the terminal

## `get-stream-settings`

This example requests the stream settings for your device for both ECG and acceleration data types

## `print-everything`

This example prints battery, hr, acc, ecg and setting data

## `multi-threaded`

This is a simple example that runs the event loop on its own thread and uses channels to allow commands from the 
terminal to stop measurement during runtime. Pass device ID as a command line argument.

## `cli-app`

This is an example of how a cli app can work. It allows the user to add/remove data types while the program is running,
start and stop measurement at will and dump all the output into a file. For simplicity it only uses PMD measurement
types.
