from worker import acquire_once

assert acquire_once(), "a single request must acquire a slot"
