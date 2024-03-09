use std::cmp::Ordering;

use crate::{
    error::{TemplateResult, TemplateKVError},
    iterator::Iterator,
};

/// A concatenated iterator contains an original iterator `origin` and a `DerivedIterFactory`.
/// New derived iterator is generated by `factory(origin.value())`.
/// The origin Iterator should yield out the last key but not the first.
/// This is just like a bucket iterator with lazy generator.
pub struct ConcatenateIterator<I: Iterator, F: DerivedIterFactory> {
    origin: I,
    factory: F,
    derived: Option<F::Iter>,
    prev_derived_value: Vec<u8>,
    err: Option<TemplateKVError>,
}

/// A factory that takes value from the origin and
pub trait DerivedIterFactory {
    type Iter: Iterator;

    /// Create a new `Iterator` based on value yield by original `Iterator`
    fn derive(&self, value: &[u8]) -> TemplateResult<Self::Iter>;
}

impl<I: Iterator, F: DerivedIterFactory> ConcatenateIterator<I, F> {
    pub fn new(origin: I, factory: F) -> Self {
        Self {
            origin,
            factory,
            derived: None,
            prev_derived_value: vec![],
            err: None,
        }
    }

    #[inline]
    fn maybe_save_err(old: &mut Option<TemplateKVError>, new: TemplateResult<()>) {
        if old.is_none() {
            if let Err(e) = new {
                *old = Some(e);
            }
        }
    }

    // Create a derived iter from the current value of the origin iter.
    // Only works when current derived iter is `None` or the previous origin value has been changed.
    // Same as `InitDataBlock` in C++ implementation
    fn init_derived_iter(&mut self) {
        if !self.origin.valid() {
            self.derived = None
        } else {
            let v = self.origin.value();
            if self.derived.is_none()
                || v.cmp(self.prev_derived_value.as_slice()) != Ordering::Equal
            {
                match self.factory.derive(v) {
                    Ok(derived) => {
                        if derived.valid() {
                            self.prev_derived_value = v.to_vec();
                        }
                        self.set_derived(Some(derived))
                    }
                    Err(e) => {
                        Self::maybe_save_err(&mut self.err, Err(e));
                        self.set_derived(None);
                    }
                }
            }
        }
    }

    // Same as `SetDataIterator` in C++ implementation
    #[inline]
    fn set_derived(&mut self, iter: Option<F::Iter>) {
        if let Some(iter) = &mut self.derived {
            Self::maybe_save_err(&mut self.err, iter.status())
        }
        self.derived = iter
    }

    // Skip invalid results util finding a valid derived iter by `next()`
    // If found, set derived iter to the first
    fn skip_forward(&mut self) {
        while self.derived.is_none() || !self.derived.as_ref().unwrap().valid() {
            if !self.origin.valid() {
                self.set_derived(None);
                break;
            }
            self.origin.next();
            self.init_derived_iter();
            if let Some(i) = &mut self.derived {
                // init to the first
                i.seek_to_first();
            }
        }
    }

    // Skip invalid results util finding a valid derived iter by `prev()`
    // If found, set derived iter to the last
    fn skip_backward(&mut self) {
        while self.derived.is_none() || !self.derived.as_ref().unwrap().valid() {
            if !self.origin.valid() {
                self.set_derived(None);
                break;
            }
            self.origin.prev();
            self.init_derived_iter();
            if let Some(i) = &mut self.derived {
                // init to the last
                i.seek_to_last();
            }
        }
    }

    #[inline]
    fn valid_or_panic(&self) {
        assert!(
            self.valid(),
            "[concatenated iterator] invalid derived iterator"
        )
    }
}

impl<I: Iterator, F: DerivedIterFactory> Iterator for ConcatenateIterator<I, F> {
    fn valid(&self) -> bool {
        if let Some(e) = &self.err {
            error!("[concatenated iter] Error: {:?}", e);
            return false;
        }
        if let Some(di) = &self.derived {
            di.valid()
        } else {
            false
        }
    }

    fn seek_to_first(&mut self) {
        self.origin.seek_to_first();
        self.init_derived_iter();
        if let Some(di) = self.derived.as_mut() {
            di.seek_to_first();
        }
        // scan forward util finding the first valid entry
        self.skip_forward();
    }

    fn seek_to_last(&mut self) {
        self.origin.seek_to_last();
        self.init_derived_iter();
        if let Some(di) = self.derived.as_mut() {
            di.seek_to_last()
        }
        // scan backward util finding the first valid entry
        self.skip_backward();
    }

    fn seek(&mut self, target: &[u8]) {
        self.origin.seek(target);
        self.init_derived_iter();
        if let Some(di) = self.derived.as_mut() {
            di.seek(target)
        }
        self.skip_forward();
    }

    fn next(&mut self) {
        self.valid_or_panic();
        let _ = self.derived.as_mut().map_or((), |di| di.next());
        self.skip_forward();
    }

    fn prev(&mut self) {
        self.valid_or_panic();
        let _ = self.derived.as_mut().map_or((), |di| di.prev());
        self.skip_backward();
    }

    fn key(&self) -> &[u8] {
        self.valid_or_panic();
        self.derived.as_ref().unwrap().key()
    }

    fn value(&self) -> &[u8] {
        self.valid_or_panic();
        self.derived.as_ref().unwrap().value()
    }

    fn status(&mut self) -> TemplateResult<()> {
        self.origin.status()?;
        if let Some(di) = self.derived.as_mut() {
            di.status()?
        };
        if let Some(e) = self.err.take() {
            return Err(e);
        }
        Ok(())
    }
}
