# Channel plugin

A minimal PiPan plugin for the [reference producer](../channel-reference). Its id is the producer's channel id, `acme.demo`.

It shows three things:

- A custom panel (`demo-status`) that subscribes to the producer's `graph` topic and sends a `refresh` intent.
- A `messageRenderers` entry that draws the producer's `note` log messages in the chat.
- An `onRequest` handler that renders the `mystery` request kind (`/demo-unknown`) with its own buttons.

The manifest also declares the graph topic under `contributes.channels`, which gives the built-in graph panel a fixed title and lists it in the panel picker.

Install it with **Install from Folder** in the Extensions tab, then run the reference producer in a session.
