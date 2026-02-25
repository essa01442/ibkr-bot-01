import time

class HealthMonitor:
    def __init__(self):
        self.last_heartbeat = time.time()
        self.status = "OK"

    def tick(self):
        self.last_heartbeat = time.time()
        self.status = "OK"

    def check(self):
        if time.time() - self.last_heartbeat > 5:
            self.status = "DEGRADED"
        return self.status
