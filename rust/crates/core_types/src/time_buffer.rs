use std::vec::Vec;

#[derive(Debug, Clone)]
pub struct TimeRingBuffer<T> {
    buffer: Vec<Option<(u64, T)>>,
    head: usize,
    tail: usize,
    capacity: usize,
    window_micros: u64,
}

impl<T> TimeRingBuffer<T> {
    pub fn new(max_elements: usize, window_micros: u64) -> Self {
        let capacity = max_elements + 1; // +1 to allow distinction between full/empty
        let mut buffer = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            buffer.push(None);
        }

        Self {
            buffer,
            head: 0,
            tail: 0,
            capacity,
            window_micros,
        }
    }

    pub fn push(&mut self, timestamp: u64, item: T) {
        let next_head = (self.head + 1) % self.capacity;

        if next_head == self.tail {
            // Buffer Full: Evict oldest (tail) O(1)
            self.tail = (self.tail + 1) % self.capacity;
        }

        self.buffer[self.head] = Some((timestamp, item));
        self.head = next_head;
    }

    pub fn prune_expired(&mut self, current_time: u64) {
        if self.head == self.tail {
            return;
        }

        let cutoff = current_time.saturating_sub(self.window_micros);

        while self.head != self.tail {
            let idx = self.tail;
            if let Some((ts, _)) = &self.buffer[idx] {
                if *ts < cutoff {
                    // Item expired, move tail
                    self.tail = (self.tail + 1) % self.capacity;
                } else {
                    // Found valid item, stop pruning
                    break;
                }
            } else {
                // If None found at tail (should not happen in normal op), skip
                self.tail = (self.tail + 1) % self.capacity;
            }
        }
    }

    pub fn len(&self) -> usize {
        if self.head >= self.tail {
            self.head - self.tail
        } else {
            self.capacity + self.head - self.tail
        }
    }

    pub fn is_empty(&self) -> bool {
        self.head == self.tail
    }

    pub fn iter(&self) -> Iter<'_, T> {
        Iter {
            buffer: &self.buffer,
            current: self.tail,
            head: self.head,
            capacity: self.capacity,
        }
    }

    pub fn min_max(&self) -> (Option<T>, Option<T>)
    where
        T: PartialOrd + Copy,
    {
        if self.is_empty() {
            return (None, None);
        }

        let mut min_val: Option<T> = None;
        let mut max_val: Option<T> = None;

        for item in self.iter() {
            match min_val {
                None => min_val = Some(*item),
                Some(m) if *item < m => min_val = Some(*item),
                _ => {}
            }
            match max_val {
                None => max_val = Some(*item),
                Some(m) if *item > m => max_val = Some(*item),
                _ => {}
            }
        }
        (min_val, max_val)
    }
}

// Iterator implementation
pub struct Iter<'a, T> {
    buffer: &'a Vec<Option<(u64, T)>>,
    current: usize,
    head: usize,
    capacity: usize,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current == self.head {
            return None;
        }

        let idx = self.current;
        self.current = (self.current + 1) % self.capacity;

        match &self.buffer[idx] {
            Some((_, val)) => Some(val),
            None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capacity_overflow() {
        let max_elements = 3;
        let mut rb = TimeRingBuffer::new(max_elements, 1000);

        rb.push(100, "A");
        rb.push(200, "B");
        rb.push(300, "C");
        assert_eq!(rb.len(), 3);

        // Push 4th element, should overwrite "A"
        rb.push(400, "D");
        assert_eq!(rb.len(), 3);

        let items: Vec<&&str> = rb.iter().collect();
        assert_eq!(items, vec![&"B", &"C", &"D"]);
    }

    #[test]
    fn test_time_eviction() {
        let max_elements = 10;
        let window = 50;
        let mut rb = TimeRingBuffer::new(max_elements, window);

        rb.push(100, 1);
        rb.push(110, 2);
        rb.push(120, 3);
        rb.push(140, 4);

        // Current time 160. Window 50. Cutoff = 110.
        // Elements < 110 are expired (100).
        // 110 is NOT < 110 (it is equal, so valid).

        rb.prune_expired(160);

        let items: Vec<&i32> = rb.iter().collect();
        assert_eq!(items, vec![&2, &3, &4]); // 1 expired

        // Current time 200. Window 50. Cutoff = 150.
        // Expired: 110, 120, 140. All gone.
        rb.prune_expired(200);
        assert!(rb.is_empty());
    }

    #[test]
    fn test_empty_behavior() {
        let rb: TimeRingBuffer<i32> = TimeRingBuffer::new(5, 100);
        assert!(rb.is_empty());
        assert_eq!(rb.len(), 0);
        assert!(rb.iter().next().is_none());
    }
}
