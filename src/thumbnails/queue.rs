//! Cola de trabajo priorizada: primero lo que la UI quiere mostrar (más cercano al foco),
//! luego indexar el resto de la carpeta empezando alrededor del foco.
use std::collections::{BTreeSet, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Job {
    /// La UI necesita la imagen decodificada.
    Display(usize),
    /// Solo generar y guardar en el índice.
    Index(usize),
}

impl Job {
    pub fn index(self) -> usize {
        match self {
            Job::Display(i) | Job::Index(i) => i,
        }
    }
}

#[derive(Debug)]
pub struct Queue {
    indexed: Vec<bool>,
    failed: Vec<bool>,
    wanted: BTreeSet<usize>,
    in_flight: HashSet<usize>,
    focus: usize,
    done: usize,
}

impl Queue {
    /// `indexed[i]` indica si la miniatura `i` ya existe en el índice en disco.
    pub fn new(indexed: Vec<bool>) -> Self {
        let done = indexed.iter().filter(|&&b| b).count();
        Self {
            failed: vec![false; indexed.len()],
            indexed,
            wanted: BTreeSet::new(),
            in_flight: HashSet::new(),
            focus: 0,
            done,
        }
    }

    pub fn set_focus(&mut self, focus: usize) {
        self.focus = focus;
    }

    /// Reemplaza lo que la UI quiere ver; ignora índices fuera de rango o fallidos.
    pub fn set_wanted(&mut self, wanted: impl IntoIterator<Item = usize>) {
        self.wanted = wanted
            .into_iter()
            .filter(|&i| i < self.indexed.len() && !self.failed[i])
            .collect();
    }

    pub fn next(&mut self) -> Option<Job> {
        let job = self.next_display().or_else(|| self.next_index())?;
        self.in_flight.insert(job.index());
        Some(job)
    }

    fn next_display(&self) -> Option<Job> {
        self.wanted
            .iter()
            .copied()
            .filter(|i| !self.in_flight.contains(i))
            .min_by_key(|&i| (i.abs_diff(self.focus), i))
            .map(Job::Display)
    }

    fn next_index(&self) -> Option<Job> {
        let n = self.indexed.len();
        if n == 0 {
            return None;
        }
        let focus = self.focus.min(n - 1);
        let pending =
            |i: usize| !self.indexed[i] && !self.failed[i] && !self.in_flight.contains(&i);
        for d in 0..n {
            if d <= focus && pending(focus - d) {
                return Some(Job::Index(focus - d));
            }
            if d > 0 && focus + d < n && pending(focus + d) {
                return Some(Job::Index(focus + d));
            }
            if d > focus && focus + d >= n {
                break;
            }
        }
        None
    }

    /// `ok = false` marca el archivo como fallido: no se reintenta en esta sesión.
    pub fn finish(&mut self, job: Job, ok: bool) {
        let i = job.index();
        self.in_flight.remove(&i);
        if matches!(job, Job::Display(_)) || !ok {
            self.wanted.remove(&i);
        }
        if i >= self.indexed.len() {
            return;
        }
        let was_done = self.indexed[i] || self.failed[i];
        if ok {
            self.indexed[i] = true;
        } else {
            self.failed[i] = true;
        }
        if !was_done {
            self.done += 1;
        }
    }

    /// Mostrada pero sin indexar: deja de estar en curso y vuelve a quedar pendiente de `Index`.
    pub fn release(&mut self, job: Job) {
        self.in_flight.remove(&job.index());
        self.wanted.remove(&job.index());
    }

    #[cfg(test)]
    pub fn is_failed(&self, i: usize) -> bool {
        self.failed.get(i).copied().unwrap_or(false)
    }

    /// (indexadas o fallidas, total)
    pub fn progress(&self) -> (usize, usize) {
        (self.done, self.indexed.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drain(q: &mut Queue) -> Vec<Job> {
        let mut jobs = Vec::new();
        while let Some(job) = q.next() {
            jobs.push(job);
            q.finish(job, true);
        }
        jobs
    }

    #[test]
    fn wanted_items_come_first_nearest_to_focus() {
        let mut q = Queue::new(vec![false; 10]);
        q.set_focus(5);
        q.set_wanted([1, 4, 8]);
        assert_eq!(q.next(), Some(Job::Display(4)));
        assert_eq!(q.next(), Some(Job::Display(8)));
        assert_eq!(q.next(), Some(Job::Display(1)));
        assert_eq!(q.next(), Some(Job::Index(5)));
    }

    #[test]
    fn indexing_expands_outward_from_focus_and_skips_indexed() {
        let mut indexed = vec![false; 6];
        indexed[3] = true;
        let mut q = Queue::new(indexed);
        q.set_focus(3);
        let order: Vec<usize> = drain(&mut q).into_iter().map(Job::index).collect();
        assert_eq!(order, vec![2, 4, 1, 5, 0]);
        assert_eq!(q.progress(), (6, 6));
    }

    #[test]
    fn in_flight_jobs_are_not_handed_out_twice() {
        let mut q = Queue::new(vec![false; 2]);
        q.set_wanted([0]);
        let a = q.next().unwrap();
        let b = q.next().unwrap();
        assert_ne!(a.index(), b.index());
        assert_eq!(q.next(), None);
    }

    #[test]
    fn indexed_item_still_served_when_wanted_for_display() {
        let mut q = Queue::new(vec![true; 3]);
        assert_eq!(q.next(), None);
        q.set_wanted([2]);
        assert_eq!(q.next(), Some(Job::Display(2)));
        q.finish(Job::Display(2), true);
        assert_eq!(q.next(), None);
    }

    #[test]
    fn failed_items_are_not_retried_and_count_as_done() {
        let mut q = Queue::new(vec![false; 2]);
        q.set_wanted([0]);
        let job = q.next().unwrap();
        q.finish(job, false);
        assert!(q.is_failed(0));
        q.set_wanted([0]);
        assert_eq!(q.next(), Some(Job::Index(1)));
        q.finish(Job::Index(1), true);
        assert_eq!(q.progress(), (2, 2));
    }

    #[test]
    fn wanted_while_indexing_is_still_displayed_after_index_finishes() {
        let mut q = Queue::new(vec![false; 1]);
        assert_eq!(q.next(), Some(Job::Index(0)));
        q.set_wanted([0]);
        assert_eq!(q.next(), None, "en curso");
        q.finish(Job::Index(0), true);
        assert_eq!(q.next(), Some(Job::Display(0)));
    }

    #[test]
    fn released_display_job_stays_pending_for_indexing() {
        let mut q = Queue::new(vec![false; 1]);
        q.set_wanted([0]);
        assert_eq!(q.next(), Some(Job::Display(0)));
        q.release(Job::Display(0));
        assert_eq!(q.progress(), (0, 1));
        assert_eq!(q.next(), Some(Job::Index(0)), "falta indexar y calificar");
    }

    #[test]
    fn set_wanted_replaces_previous_request() {
        let mut q = Queue::new(vec![true; 5]);
        q.set_wanted([0, 1]);
        q.set_wanted([4]);
        assert_eq!(q.next(), Some(Job::Display(4)));
        assert_eq!(q.next(), None);
    }

    #[test]
    fn focus_out_of_range_is_clamped() {
        let mut q = Queue::new(vec![false; 3]);
        q.set_focus(99);
        assert_eq!(q.next(), Some(Job::Index(2)));
        let mut empty = Queue::new(vec![]);
        empty.set_focus(4);
        assert_eq!(empty.next(), None);
    }
}
