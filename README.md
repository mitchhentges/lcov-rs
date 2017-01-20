# lcov-rs

Rust re-implementation of the [`lcov` code-coverage processor](http://ltp.sourceforge.net/coverage/lcov.php). Used
by the Mozilla "A Team" for tracking code coverage for [Firefox](https://www.mozilla.org/en-US/firefox/new/).

## Goals

This won't be an `lcov` alternative with feature parity. Rather, this will minimally fulfill the role that `lcov` plays
in processing [`gcov`](https://gcc.gnu.org/onlinedocs/gcc/Gcov.html) data. See
[`gcov_to_es.py`](https://github.com/klahnakoski/ActiveData-ETL/blob/codecoverage/activedata_etl/transforms/gcov_to_es.py).

* Be faster than `lcov`. Ideally by a lot. Gotta go fast.
* Be more cross-platform than `lcov`. Processing `gcov` data on Windows is a pain, due to `lcov` being written in Perl,
and being super linux-specific.
* Be more modern than `lcov`. It's hard to find developers to contribute to `lcov` because Perl isn't used as frequently
anymore.