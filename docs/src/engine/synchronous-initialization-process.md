# Synchronous Initialization Process

This documentation describes the synchronous initialization process of the Valqeron Engine. It outlines the steps
involved in setting up and initializing the engine, ensuring that all necessary components are properly configured and
ready for use.

The main initialization process involves several key steps:

1. Engine bootstrap sequence
2. Locking and sockets initialization
3. Database initialization
4. Runtime initialization
5. Application initialization

Those steps are executed in a specific order to ensure that the engine is fully initialized and ready for use. It
implements a finite state machine (FSM).

## Engine