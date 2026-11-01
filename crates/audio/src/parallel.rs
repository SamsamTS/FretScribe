use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::thread;

/// `items.iter().map(f).collect()` spread over at most one thread per core; results keep the
/// input order. A panic in `f` propagates to the caller.
pub(crate) fn parallel_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let workers = thread::available_parallelism().map_or(1, usize::from).min(items.len());
    if workers <= 1 {
        return items.iter().map(f).collect();
    }
    let next = AtomicUsize::new(0);
    let slots: Vec<Mutex<Option<R>>> = items.iter().map(|_| Mutex::new(None)).collect();
    thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(item) = items.get(index) else {
                    break;
                };
                *slots[index].lock().unwrap_or_else(|e| e.into_inner()) = Some(f(item));
            });
        }
    });
    slots
        .into_iter()
        .filter_map(|slot| slot.into_inner().unwrap_or_else(|e| e.into_inner()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_order_and_runs_everything_once() {
        let items: Vec<usize> = (0..50).collect();
        let out = parallel_map(&items, |&i| i * 2);
        assert_eq!(out, (0..50).map(|i| i * 2).collect::<Vec<_>>());
        assert!(parallel_map::<usize, usize>(&[], |&i| i).is_empty());
        assert_eq!(parallel_map(&[7], |&i| i + 1), [8]);

        let slow_first: Vec<u64> = vec![30, 0, 0, 0, 0, 0, 0, 0];
        let out = parallel_map(&slow_first, |&ms| {
            std::thread::sleep(std::time::Duration::from_millis(ms));
            ms
        });
        assert_eq!(out, slow_first);
    }
}
