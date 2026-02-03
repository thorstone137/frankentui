#![forbid(unsafe_code)]

//! Subscription system for continuous event sources.
//!
//! Subscriptions provide a declarative way to receive events from external
//! sources like timers, file watchers, or network connections. The runtime
//! manages subscription lifecycles automatically based on what the model
//! declares as active.
//!
//! # How it works
//!
//! 1. `Model::subscriptions()` returns the set of active subscriptions
//! 2. After each `update()`, the runtime compares active vs previous subscriptions
//! 3. New subscriptions are started, removed ones are stopped
//! 4. Subscription messages are routed through `Model::update()`

use std::collections::HashSet;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// A unique identifier for a subscription.
///
/// Used by the runtime to track which subscriptions are active and
/// to deduplicate subscriptions across update cycles.
pub type SubId = u64;

/// A subscription produces messages from an external event source.
///
/// Subscriptions run on background threads and send messages through
/// the provided channel. The runtime manages their lifecycle.
pub trait Subscription<M: Send + 'static>: Send {
    /// Unique identifier for deduplication.
    ///
    /// Subscriptions with the same ID are considered identical.
    /// The runtime uses this to avoid restarting unchanged subscriptions.
    fn id(&self) -> SubId;

    /// Start the subscription, sending messages through the channel.
    ///
    /// This is called on a background thread. Implementations should
    /// loop and send messages until the channel is disconnected (receiver dropped)
    /// or the stop signal is received.
    fn run(&self, sender: mpsc::Sender<M>, stop: StopSignal);
}

/// Signal for stopping a subscription.
///
/// When the runtime stops a subscription, it sets this signal. The subscription
/// should check it periodically and exit its run loop when set.
#[derive(Clone)]
pub struct StopSignal {
    inner: std::sync::Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>,
}

impl StopSignal {
    /// Create a new stop signal pair (signal, trigger).
    pub(crate) fn new() -> (Self, StopTrigger) {
        let inner = std::sync::Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));
        let signal = Self {
            inner: inner.clone(),
        };
        let trigger = StopTrigger { inner };
        (signal, trigger)
    }

    /// Check if the stop signal has been triggered.
    pub fn is_stopped(&self) -> bool {
        let (lock, _) = &*self.inner;
        *lock.lock().unwrap()
    }

    /// Wait for either the stop signal or a timeout.
    ///
    /// Returns `true` if stopped, `false` if timed out.
    /// Blocks the thread efficiently using a condition variable.
    /// Handles spurious wakeups by looping until condition met or timeout expired.
    pub fn wait_timeout(&self, duration: Duration) -> bool {
        let (lock, cvar) = &*self.inner;
        let mut stopped = lock.lock().unwrap();
        if *stopped {
            return true;
        }

        let start = std::time::Instant::now();
        let mut remaining = duration;

        loop {
            let (guard, result) = cvar.wait_timeout(stopped, remaining).unwrap();
            stopped = guard;
            if *stopped {
                return true;
            }
            if result.timed_out() {
                return false;
            }
            // Check if we really timed out (spurious wakeup protection)
            let elapsed = start.elapsed();
            if elapsed >= duration {
                return false;
            }
            remaining = duration - elapsed;
        }
    }
}

/// Trigger to stop a subscription from the runtime side.
pub(crate) struct StopTrigger {
    inner: std::sync::Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>,
}

impl StopTrigger {
    /// Signal the subscription to stop.
    pub(crate) fn stop(&self) {
        let (lock, cvar) = &*self.inner;
        let mut stopped = lock.lock().unwrap();
        *stopped = true;
        cvar.notify_all();
    }
}

/// A running subscription handle.
pub(crate) struct RunningSubscription {
    pub(crate) id: SubId,
    trigger: StopTrigger,
    thread: Option<thread::JoinHandle<()>>,
}

impl RunningSubscription {
    /// Stop the subscription and join its thread.
    pub(crate) fn stop(mut self) {
        self.trigger.stop();
        if let Some(handle) = self.thread.take() {
            // Give the thread a moment to finish, but don't block forever
            let _ = handle.join();
        }
    }
}

impl Drop for RunningSubscription {
    fn drop(&mut self) {
        self.trigger.stop();
        // Don't join in drop to avoid blocking
    }
}

/// Manages the lifecycle of subscriptions for a program.
pub(crate) struct SubscriptionManager<M: Send + 'static> {
    active: Vec<RunningSubscription>,
    sender: mpsc::Sender<M>,
    receiver: mpsc::Receiver<M>,
}

impl<M: Send + 'static> SubscriptionManager<M> {
    pub(crate) fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            active: Vec::new(),
            sender,
            receiver,
        }
    }

    /// Update the set of active subscriptions.
    ///
    /// Compares the new set against currently running subscriptions:
    /// - Starts subscriptions that are new (ID not in active set)
    /// - Stops subscriptions that are no longer declared (ID not in new set)
    /// - Leaves unchanged subscriptions running
    pub(crate) fn reconcile(&mut self, subscriptions: Vec<Box<dyn Subscription<M>>>) {
        let new_ids: HashSet<SubId> = subscriptions.iter().map(|s| s.id()).collect();
        let active_count_before = self.active.len();

        crate::debug_trace!(
            "reconcile: new_ids={:?}, active_before={}",
            new_ids,
            active_count_before
        );
        tracing::trace!(
            new_id_count = new_ids.len(),
            active_before = active_count_before,
            new_ids = ?new_ids,
            "subscription reconcile starting"
        );

        // Stop subscriptions that are no longer active
        let mut remaining = Vec::new();
        for running in self.active.drain(..) {
            if new_ids.contains(&running.id) {
                remaining.push(running);
            } else {
                crate::debug_trace!("stopping subscription: id={}", running.id);
                tracing::debug!(sub_id = running.id, "Stopping subscription");
                running.stop();
            }
        }
        self.active = remaining;

        // Start new subscriptions
        let mut active_ids: HashSet<SubId> = self.active.iter().map(|r| r.id).collect();
        for sub in subscriptions {
            let id = sub.id();
            if !active_ids.insert(id) {
                continue;
            }

            crate::debug_trace!("starting subscription: id={}", id);
            tracing::debug!(sub_id = id, "Starting subscription");
            let (signal, trigger) = StopSignal::new();
            let sender = self.sender.clone();

            let thread = thread::spawn(move || {
                sub.run(sender, signal);
            });

            self.active.push(RunningSubscription {
                id,
                trigger,
                thread: Some(thread),
            });
        }

        let active_count_after = self.active.len();
        crate::debug_trace!(
            "reconcile complete: active_after={}",
            active_count_after
        );
        tracing::trace!(
            active_before = active_count_before,
            active_after = active_count_after,
            started = active_count_after.saturating_sub(active_count_before),
            stopped = active_count_before.saturating_sub(active_count_after),
            "subscription reconcile complete"
        );
    }

    /// Drain pending messages from subscriptions.
    pub(crate) fn drain_messages(&self) -> Vec<M> {
        let mut messages = Vec::new();
        while let Ok(msg) = self.receiver.try_recv() {
            messages.push(msg);
        }
        messages
    }

    /// Return the number of active subscriptions.
    #[inline]
    pub(crate) fn active_count(&self) -> usize {
        self.active.len()
    }

    /// Stop all running subscriptions.
    pub(crate) fn stop_all(&mut self) {
        for running in self.active.drain(..) {
            running.stop();
        }
    }
}

impl<M: Send + 'static> Drop for SubscriptionManager<M> {
    fn drop(&mut self) {
        self.stop_all();
    }
}

// --- Built-in subscriptions ---

/// A subscription that fires at a fixed interval.
///
/// # Example
///
/// ```ignore
/// fn subscriptions(&self) -> Vec<Box<dyn Subscription<MyMsg>>> {
///     vec![Box::new(Every::new(Duration::from_secs(1), || MyMsg::Tick))]
/// }
/// ```
pub struct Every<M: Send + 'static> {
    id: SubId,
    interval: Duration,
    make_msg: Box<dyn Fn() -> M + Send + Sync>,
}

impl<M: Send + 'static> Every<M> {
    /// Create a tick subscription with the given interval and message factory.
    pub fn new(interval: Duration, make_msg: impl Fn() -> M + Send + Sync + 'static) -> Self {
        // Generate a stable ID from the interval to allow deduplication
        let id = interval.as_nanos() as u64 ^ 0x5449_434B; // "TICK" magic
        Self {
            id,
            interval,
            make_msg: Box::new(make_msg),
        }
    }

    /// Create a tick subscription with an explicit ID.
    pub fn with_id(
        id: SubId,
        interval: Duration,
        make_msg: impl Fn() -> M + Send + Sync + 'static,
    ) -> Self {
        Self {
            id,
            interval,
            make_msg: Box::new(make_msg),
        }
    }
}

impl<M: Send + 'static> Subscription<M> for Every<M> {
    fn id(&self) -> SubId {
        self.id
    }

    fn run(&self, sender: mpsc::Sender<M>, stop: StopSignal) {
        let mut tick_count: u64 = 0;
        crate::debug_trace!(
            "Every subscription started: id={}, interval={:?}",
            self.id,
            self.interval
        );
        loop {
            if stop.wait_timeout(self.interval) {
                crate::debug_trace!(
                    "Every subscription stopped: id={}, sent {} ticks",
                    self.id,
                    tick_count
                );
                break;
            }
            tick_count += 1;
            let msg = (self.make_msg)();
            if sender.send(msg).is_err() {
                crate::debug_trace!(
                    "Every subscription channel closed: id={}, sent {} ticks",
                    self.id,
                    tick_count
                );
                break;
            }
        }
    }
}

/// A mock subscription for testing.
///
/// Immediately sends all queued messages and then stops.
pub struct MockSubscription<M: Send + 'static> {
    id: SubId,
    messages: Vec<M>,
}

impl<M: Send + Clone + 'static> MockSubscription<M> {
    /// Create a mock subscription that sends the given messages.
    pub fn new(id: SubId, messages: Vec<M>) -> Self {
        Self { id, messages }
    }
}

impl<M: Send + Clone + 'static> Subscription<M> for MockSubscription<M> {
    fn id(&self) -> SubId {
        self.id
    }

    fn run(&self, sender: mpsc::Sender<M>, _stop: StopSignal) {
        for msg in &self.messages {
            if sender.send(msg.clone()).is_err() {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    enum TestMsg {
        Tick,
        Value(i32),
    }

    #[test]
    fn stop_signal_starts_false() {
        let (signal, _trigger) = StopSignal::new();
        assert!(!signal.is_stopped());
    }

    #[test]
    fn stop_signal_becomes_true_after_trigger() {
        let (signal, trigger) = StopSignal::new();
        trigger.stop();
        assert!(signal.is_stopped());
    }

    #[test]
    fn stop_signal_wait_returns_true_when_stopped() {
        let (signal, trigger) = StopSignal::new();
        trigger.stop();
        assert!(signal.wait_timeout(Duration::from_millis(100)));
    }

    #[test]
    fn stop_signal_wait_returns_false_on_timeout() {
        let (signal, _trigger) = StopSignal::new();
        assert!(!signal.wait_timeout(Duration::from_millis(10)));
    }

    #[test]
    fn mock_subscription_sends_messages() {
        let sub = MockSubscription::new(1, vec![TestMsg::Value(1), TestMsg::Value(2)]);
        let (tx, rx) = mpsc::channel();
        let (signal, _trigger) = StopSignal::new();

        sub.run(tx, signal);

        let msgs: Vec<_> = rx.try_iter().collect();
        assert_eq!(msgs, vec![TestMsg::Value(1), TestMsg::Value(2)]);
    }

    #[test]
    fn every_subscription_fires() {
        let sub = Every::new(Duration::from_millis(10), || TestMsg::Tick);
        let (tx, rx) = mpsc::channel();
        let (signal, trigger) = StopSignal::new();

        let handle = thread::spawn(move || {
            sub.run(tx, signal);
        });

        // Wait for a few ticks
        thread::sleep(Duration::from_millis(50));
        trigger.stop();
        handle.join().unwrap();

        let msgs: Vec<_> = rx.try_iter().collect();
        assert!(!msgs.is_empty(), "Should have received at least one tick");
        assert!(msgs.iter().all(|m| *m == TestMsg::Tick));
    }

    #[test]
    fn every_subscription_uses_stable_id() {
        let sub1 = Every::<TestMsg>::new(Duration::from_secs(1), || TestMsg::Tick);
        let sub2 = Every::<TestMsg>::new(Duration::from_secs(1), || TestMsg::Tick);
        assert_eq!(sub1.id(), sub2.id());
    }

    #[test]
    fn every_subscription_different_intervals_different_ids() {
        let sub1 = Every::<TestMsg>::new(Duration::from_secs(1), || TestMsg::Tick);
        let sub2 = Every::<TestMsg>::new(Duration::from_secs(2), || TestMsg::Tick);
        assert_ne!(sub1.id(), sub2.id());
    }

    #[test]
    fn subscription_manager_starts_subscriptions() {
        let mut mgr = SubscriptionManager::<TestMsg>::new();
        let subs: Vec<Box<dyn Subscription<TestMsg>>> =
            vec![Box::new(MockSubscription::new(1, vec![TestMsg::Value(42)]))];

        mgr.reconcile(subs);

        // Give the thread a moment to send
        thread::sleep(Duration::from_millis(20));

        let msgs = mgr.drain_messages();
        assert_eq!(msgs, vec![TestMsg::Value(42)]);
    }

    #[test]
    fn subscription_manager_dedupes_duplicate_ids() {
        let mut mgr = SubscriptionManager::<TestMsg>::new();
        let subs: Vec<Box<dyn Subscription<TestMsg>>> = vec![
            Box::new(MockSubscription::new(7, vec![TestMsg::Value(1)])),
            Box::new(MockSubscription::new(7, vec![TestMsg::Value(2)])),
        ];

        mgr.reconcile(subs);

        thread::sleep(Duration::from_millis(20));
        let msgs = mgr.drain_messages();
        assert_eq!(msgs, vec![TestMsg::Value(1)]);
    }

    #[test]
    fn subscription_manager_stops_removed() {
        let mut mgr = SubscriptionManager::<TestMsg>::new();

        // Start with one subscription
        mgr.reconcile(vec![Box::new(Every::with_id(
            99,
            Duration::from_millis(5),
            || TestMsg::Tick,
        ))]);

        thread::sleep(Duration::from_millis(20));
        let msgs_before = mgr.drain_messages();
        assert!(!msgs_before.is_empty());

        // Remove it
        mgr.reconcile(vec![]);

        // Drain any remaining buffered messages
        thread::sleep(Duration::from_millis(20));
        let _ = mgr.drain_messages();

        // After stopping, no more messages should arrive
        thread::sleep(Duration::from_millis(30));
        let msgs_after = mgr.drain_messages();
        assert!(
            msgs_after.is_empty(),
            "Should stop receiving after reconcile with empty set"
        );
    }

    #[test]
    fn subscription_manager_keeps_unchanged() {
        let mut mgr = SubscriptionManager::<TestMsg>::new();

        // Start subscription
        mgr.reconcile(vec![Box::new(Every::with_id(
            50,
            Duration::from_millis(10),
            || TestMsg::Tick,
        ))]);

        thread::sleep(Duration::from_millis(30));
        let _ = mgr.drain_messages();

        // Reconcile with same ID - should keep running
        mgr.reconcile(vec![Box::new(Every::with_id(
            50,
            Duration::from_millis(10),
            || TestMsg::Tick,
        ))]);

        thread::sleep(Duration::from_millis(30));
        let msgs = mgr.drain_messages();
        assert!(!msgs.is_empty(), "Subscription should still be running");
    }

    #[test]
    fn subscription_manager_stop_all() {
        let mut mgr = SubscriptionManager::<TestMsg>::new();

        mgr.reconcile(vec![
            Box::new(Every::with_id(1, Duration::from_millis(5), || {
                TestMsg::Value(1)
            })),
            Box::new(Every::with_id(2, Duration::from_millis(5), || {
                TestMsg::Value(2)
            })),
        ]);

        thread::sleep(Duration::from_millis(20));
        mgr.stop_all();

        thread::sleep(Duration::from_millis(20));
        let _ = mgr.drain_messages();
        thread::sleep(Duration::from_millis(30));
        let msgs = mgr.drain_messages();
        assert!(msgs.is_empty());
    }

    // =========================================================================
    // ADDITIONAL TESTS - Cmd sequencing + Subscriptions (bd-2nu8.10.2)
    // =========================================================================

    #[test]
    fn stop_signal_is_cloneable() {
        let (signal, trigger) = StopSignal::new();
        let signal_clone = signal.clone();

        assert!(!signal.is_stopped());
        assert!(!signal_clone.is_stopped());

        trigger.stop();

        assert!(signal.is_stopped());
        assert!(signal_clone.is_stopped());
    }

    #[test]
    fn stop_signal_wait_wakes_immediately_when_already_stopped() {
        let (signal, trigger) = StopSignal::new();
        trigger.stop();

        // Should return immediately, not wait for timeout
        let start = std::time::Instant::now();
        let stopped = signal.wait_timeout(Duration::from_secs(10));
        let elapsed = start.elapsed();

        assert!(stopped);
        assert!(elapsed < Duration::from_millis(100));
    }

    #[test]
    fn stop_signal_wait_is_interrupted_by_trigger() {
        let (signal, trigger) = StopSignal::new();

        let signal_clone = signal.clone();
        let handle = thread::spawn(move || signal_clone.wait_timeout(Duration::from_secs(10)));

        // Give thread time to start waiting
        thread::sleep(Duration::from_millis(20));
        trigger.stop();

        let stopped = handle.join().unwrap();
        assert!(stopped);
    }

    #[test]
    fn mock_subscription_empty_messages() {
        let sub = MockSubscription::<TestMsg>::new(1, vec![]);
        let (tx, rx) = mpsc::channel();
        let (signal, _trigger) = StopSignal::new();

        sub.run(tx, signal);

        let msgs: Vec<_> = rx.try_iter().collect();
        assert!(msgs.is_empty());
    }

    #[test]
    fn mock_subscription_id_is_preserved() {
        let sub = MockSubscription::<TestMsg>::new(42, vec![]);
        assert_eq!(sub.id(), 42);
    }

    #[test]
    fn mock_subscription_stops_on_disconnected_receiver() {
        let sub = MockSubscription::new(
            1,
            vec![TestMsg::Value(1), TestMsg::Value(2), TestMsg::Value(3)],
        );
        let (tx, rx) = mpsc::channel();
        let (signal, _trigger) = StopSignal::new();

        // Drop receiver before running
        drop(rx);

        // Should not panic, just return
        sub.run(tx, signal);
    }

    #[test]
    fn every_with_id_preserves_custom_id() {
        let sub = Every::<TestMsg>::with_id(12345, Duration::from_secs(1), || TestMsg::Tick);
        assert_eq!(sub.id(), 12345);
    }

    #[test]
    fn every_stops_on_disconnected_receiver() {
        let sub = Every::new(Duration::from_millis(5), || TestMsg::Tick);
        let (tx, rx) = mpsc::channel();
        let (signal, _trigger) = StopSignal::new();

        // Drop receiver before running
        drop(rx);

        // Should exit the loop when send fails
        let handle = thread::spawn(move || {
            sub.run(tx, signal);
        });

        // Should complete quickly, not hang
        let result = handle.join();
        assert!(result.is_ok());
    }

    #[test]
    fn every_respects_interval() {
        let sub = Every::with_id(1, Duration::from_millis(50), || TestMsg::Tick);
        let (tx, rx) = mpsc::channel();
        let (signal, trigger) = StopSignal::new();

        let start = std::time::Instant::now();
        let handle = thread::spawn(move || {
            sub.run(tx, signal);
        });

        // Wait for 3 ticks worth of time
        thread::sleep(Duration::from_millis(160));
        trigger.stop();
        handle.join().unwrap();

        let msgs: Vec<_> = rx.try_iter().collect();
        let elapsed = start.elapsed();

        // Should have approximately 3 ticks (at 50ms intervals over 160ms)
        assert!(
            msgs.len() >= 2,
            "Expected at least 2 ticks, got {}",
            msgs.len()
        );
        assert!(
            msgs.len() <= 4,
            "Expected at most 4 ticks, got {}",
            msgs.len()
        );
        assert!(elapsed >= Duration::from_millis(150));
    }

    #[test]
    fn subscription_manager_empty_reconcile() {
        let mut mgr = SubscriptionManager::<TestMsg>::new();

        // Reconcile with empty list should not panic
        mgr.reconcile(vec![]);
        let msgs = mgr.drain_messages();
        assert!(msgs.is_empty());
    }

    #[test]
    fn subscription_manager_drain_messages_returns_all() {
        let mut mgr = SubscriptionManager::<TestMsg>::new();
        let subs: Vec<Box<dyn Subscription<TestMsg>>> = vec![Box::new(MockSubscription::new(
            1,
            vec![TestMsg::Value(1), TestMsg::Value(2)],
        ))];

        mgr.reconcile(subs);
        thread::sleep(Duration::from_millis(20));

        let msgs = mgr.drain_messages();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0], TestMsg::Value(1));
        assert_eq!(msgs[1], TestMsg::Value(2));

        // Second drain should be empty
        let msgs2 = mgr.drain_messages();
        assert!(msgs2.is_empty());
    }

    #[test]
    fn subscription_manager_replaces_subscription_with_different_id() {
        let mut mgr = SubscriptionManager::<TestMsg>::new();

        // Start with ID 1
        mgr.reconcile(vec![Box::new(MockSubscription::new(
            1,
            vec![TestMsg::Value(1)],
        ))]);
        thread::sleep(Duration::from_millis(20));
        let msgs1 = mgr.drain_messages();
        assert_eq!(msgs1, vec![TestMsg::Value(1)]);

        // Replace with ID 2
        mgr.reconcile(vec![Box::new(MockSubscription::new(
            2,
            vec![TestMsg::Value(2)],
        ))]);
        thread::sleep(Duration::from_millis(20));
        let msgs2 = mgr.drain_messages();
        assert_eq!(msgs2, vec![TestMsg::Value(2)]);
    }

    #[test]
    fn subscription_manager_multiple_subscriptions() {
        let mut mgr = SubscriptionManager::<TestMsg>::new();
        let subs: Vec<Box<dyn Subscription<TestMsg>>> = vec![
            Box::new(MockSubscription::new(1, vec![TestMsg::Value(10)])),
            Box::new(MockSubscription::new(2, vec![TestMsg::Value(20)])),
            Box::new(MockSubscription::new(3, vec![TestMsg::Value(30)])),
        ];

        mgr.reconcile(subs);
        thread::sleep(Duration::from_millis(30));

        let mut msgs = mgr.drain_messages();
        msgs.sort_by_key(|m| match m {
            TestMsg::Value(v) => *v,
            _ => 0,
        });

        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0], TestMsg::Value(10));
        assert_eq!(msgs[1], TestMsg::Value(20));
        assert_eq!(msgs[2], TestMsg::Value(30));
    }

    #[test]
    fn subscription_manager_partial_update() {
        let mut mgr = SubscriptionManager::<TestMsg>::new();

        // Start with 3 subscriptions
        mgr.reconcile(vec![
            Box::new(Every::with_id(1, Duration::from_millis(10), || {
                TestMsg::Value(1)
            })),
            Box::new(Every::with_id(2, Duration::from_millis(10), || {
                TestMsg::Value(2)
            })),
            Box::new(Every::with_id(3, Duration::from_millis(10), || {
                TestMsg::Value(3)
            })),
        ]);

        thread::sleep(Duration::from_millis(30));
        let _ = mgr.drain_messages();

        // Remove subscription 2, keep 1 and 3
        mgr.reconcile(vec![
            Box::new(Every::with_id(1, Duration::from_millis(10), || {
                TestMsg::Value(1)
            })),
            Box::new(Every::with_id(3, Duration::from_millis(10), || {
                TestMsg::Value(3)
            })),
        ]);

        // Drain any in-flight messages that were sent before the stop signal was processed.
        // This clears the race window between stop signal and message send.
        let _ = mgr.drain_messages();

        // Now wait for new messages from the remaining subscriptions
        thread::sleep(Duration::from_millis(30));
        let msgs = mgr.drain_messages();

        // Should only have values 1 and 3, not 2
        let values: Vec<i32> = msgs
            .iter()
            .filter_map(|m| match m {
                TestMsg::Value(v) => Some(*v),
                _ => None,
            })
            .collect();

        assert!(
            values.contains(&1),
            "Should still receive from subscription 1"
        );
        assert!(
            values.contains(&3),
            "Should still receive from subscription 3"
        );
        assert!(
            !values.contains(&2),
            "Should not receive from stopped subscription 2"
        );
    }

    #[test]
    fn subscription_manager_drop_stops_all() {
        let (_signal, _) = StopSignal::new();
        let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag_clone = flag.clone();

        struct FlagSubscription {
            id: SubId,
            flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
        }

        impl Subscription<TestMsg> for FlagSubscription {
            fn id(&self) -> SubId {
                self.id
            }

            fn run(&self, _sender: mpsc::Sender<TestMsg>, stop: StopSignal) {
                while !stop.is_stopped() {
                    thread::sleep(Duration::from_millis(5));
                }
                self.flag.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        }

        {
            let mut mgr = SubscriptionManager::<TestMsg>::new();
            mgr.reconcile(vec![Box::new(FlagSubscription {
                id: 1,
                flag: flag_clone,
            })]);

            thread::sleep(Duration::from_millis(20));
            // mgr drops here, should stop all subscriptions
        }

        thread::sleep(Duration::from_millis(50));
        assert!(
            flag.load(std::sync::atomic::Ordering::SeqCst),
            "Subscription should have stopped on drop"
        );
    }

    #[test]
    fn running_subscription_stop_joins_thread() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let completed = std::sync::Arc::new(AtomicBool::new(false));
        let completed_clone = completed.clone();

        let (signal, trigger) = StopSignal::new();
        let (_tx, _rx) = mpsc::channel::<TestMsg>();

        let thread = thread::spawn(move || {
            while !signal.is_stopped() {
                thread::sleep(Duration::from_millis(5));
            }
            completed_clone.store(true, Ordering::SeqCst);
        });

        let running = RunningSubscription {
            id: 1,
            trigger,
            thread: Some(thread),
        };

        running.stop();
        assert!(completed.load(Ordering::SeqCst));
    }

    #[test]
    fn every_id_stable_across_instances() {
        // Same interval should produce same ID
        let sub1 = Every::<TestMsg>::new(Duration::from_millis(100), || TestMsg::Tick);
        let sub2 = Every::<TestMsg>::new(Duration::from_millis(100), || TestMsg::Tick);
        let sub3 = Every::<TestMsg>::new(Duration::from_millis(100), || TestMsg::Value(1));

        assert_eq!(sub1.id(), sub2.id());
        assert_eq!(sub2.id(), sub3.id()); // ID is based on interval, not message factory
    }

    #[test]
    fn drain_messages_preserves_order() {
        let mut mgr = SubscriptionManager::<TestMsg>::new();

        // Use a custom subscription that sends messages in order
        struct OrderedSubscription {
            values: Vec<i32>,
        }

        impl Subscription<TestMsg> for OrderedSubscription {
            fn id(&self) -> SubId {
                999
            }

            fn run(&self, sender: mpsc::Sender<TestMsg>, _stop: StopSignal) {
                for v in &self.values {
                    let _ = sender.send(TestMsg::Value(*v));
                    thread::sleep(Duration::from_millis(1));
                }
            }
        }

        mgr.reconcile(vec![Box::new(OrderedSubscription {
            values: vec![1, 2, 3, 4, 5],
        })]);

        thread::sleep(Duration::from_millis(30));
        let msgs = mgr.drain_messages();

        let values: Vec<i32> = msgs
            .iter()
            .filter_map(|m| match m {
                TestMsg::Value(v) => Some(*v),
                _ => None,
            })
            .collect();

        assert_eq!(values, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn subscription_manager_new_is_empty() {
        let mgr = SubscriptionManager::<TestMsg>::new();
        let msgs = mgr.drain_messages();
        assert!(msgs.is_empty());
    }
}
