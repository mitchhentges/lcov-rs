# lcov-rs (Codename `bohemian-waxwing`) [![Build Status](https://travis-ci.org/mitchhentges/lcov-rs.svg?branch=master)](https://travis-ci.org/mitchhentges/lcov-rs)

Rust re-implementation of the [`lcov` code-coverage processor](http://ltp.sourceforge.net/coverage/lcov.php). Used
by the Mozilla "A Team" for tracking code coverage for [Firefox](https://www.mozilla.org/en-US/firefox/new/).

## Goals

This won't be an `lcov` alternative with feature parity. Rather, this will minimally fulfill the role that `lcov` plays
in processing [`gcov`](https://gcc.gnu.org/onlinedocs/gcc/Gcov.html) data. See
[`gcov_to_es.py`](https://github.com/klahnakoski/ActiveData-ETL/blob/codecoverage/activedata_etl/transforms/gcov_to_es.py).

* API parity with `lcov`: read from the same `.gcda`/`.gcno` files, produce the same `.info` file
* Be faster than `lcov`. Ideally by a lot. Gotta go fast.
* Be more cross-platform than `lcov`. Processing `gcov` data on Windows is a pain, due to `lcov` being written in Perl,
and being super linux-specific.
* Be more modern than `lcov`. It's hard to find developers to contribute to `lcov` because Perl isn't used as frequently
anymore.

## Behaviour Difference from `lcov`

`lcov` invokes `gcov` on `.gcda` and `.gcno` files, then reads the resulting `.gcov` files and scrapes them for the
required data. When invoked, `gcov` finds the original source files to produce the `.gcov` file, that looks like this:

```
        -:    0:Source:/home/mitch/dev/mozilla-central/gfx/2d/2D.h
        -:    0:Graph:/home/mitch/dev/mozilla-central/obj-x86_64-pc-linux-gnu/gfx/2d/DrawTargetSkia.gcno
        -:    0:Data:/home/mitch/dev/mozilla-central/obj-x86_64-pc-linux-gnu/gfx/2d/DrawTargetSkia.gcda
        -:    0:Runs:2
        -:    0:Programs:1
        -:    1:/* -*- Mode: C++; tab-width: 20; indent-tabs-mode: nil; c-basic-offset: 2 -*-
        -:    2: * This Source Code Form is subject to the terms of the Mozilla Public
        -:    3: * License, v. 2.0. If a copy of the MPL was not distributed with this
        -:    4: * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
        -:    5:
        -:    6:#ifndef _MOZILLA_GFX_2D_H
        -:    7:#define _MOZILLA_GFX_2D_H
        -:    8:
        -:    9:#include "Types.h"
        -:   10:#include "Point.h"
        -:   11:#include "Rect.h"
        -:   12:#include "Matrix.h"
        -:   13:#include "Quaternion.h"
        -:   14:#include "UserData.h"
        -:   15:#include <vector>
... <snip>
function _ZNK7mozilla3gfx12ColorPattern7GetTypeEv called 0 returned 0% blocks executed 0%
    #####:  208:  virtual PatternType GetType() const override
... <snip>
```
Note that the source is copied from the original file (and `gcov` complains loudly when it can't find the original
source... but still runs, filling in `/*EOF*/` per line). When `lcov` reads this file shortly afterward, it literally
ignores all the source code, and performs a regex search for lines like "function ... called", and then processes that.

Meanwhile, `lcov-rs` will only read the `.gcda` and `.gcno` files, and will produce the `.info` files from that, without
any intermediate files, and without any invocation of intermediate tools (like `gcov`).

### Optimizations

* `lcov-rs` will read directly from the `.gcda` and `.gcno` files. This will avoid attempting to read the source
code file, then throwing the results away.
* Don't save intermediate files. `lcov` uses `gcov` data by invoking it, then reading the intermediate `.gcov` files,
deleting them afterwards. `lcov-rs` will read data directly from the `.gcda` and `.gcno` files, and perform any
processing on its own, in memory.
* Read multiple files in parallel, avoiding blocking on file-read as much as possible. Currently, `lcov` reads each
file sequentially, and blocks. `lcov-rs` will be "preloading" the next `.gcda` and `.gcno` files from disk while
processing the "current" files. Additionally, `lcov-rs` will work in parallel, parsing and refining the raw data in
parallel.

## Why is the codename `bohemian-waxwing`?

I like naming projects after birds (see [Turaco](https://github.com/mitchhentges/turaco#why-is-this-called-turaco) for
proof). `bohemian-waxwing` was started in Calgary, Canada, and Bohemian Waxwings exist in Calgary.

![bohemian-waxwing](bohemian-waxwing.jpg)