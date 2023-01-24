# Hassium, the Windows desktop un-shuffler

## What is Hassium?
It's a simple program that simply un-shuffles your Desktop when Windows decides to shuffle it.

## Why?
If you've had the misfortune of using Windows 10, and especially if you have a multi-monitor setup over DisplayPort,
you've probably experienced something along the lines of this:

➜ Go AFK, or even to sleep
➜ Monitors go to sleep
➜ Monitors wake up
➜ Windows are shuffled around

This issue comes from Windows' handling of DisplayPort monitors, and is a known issue. It's been around for years, and
has allegedly been fixed in Windows 11.

## How does Hassium work?
Fortunately, Windows' API exposes events that signal when a monitor is connected or disconnected. Hassium uses these
events to detect when a monitor is connected, and then moves all your windows back to where they were before your monitor
went to sleep.

This also makes it useful for instances where you bump a power cable and unplug your monitor, or any other event where
Windows decides to shuffle your windows around.

## Installation

Simply download the latest release from the [releases page](https://github.come/VelvetThePanda/Hassium/releases) and run it.

You can also build from source by cloning the repository and running `cargo build --release`.