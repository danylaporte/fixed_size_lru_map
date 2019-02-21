[![Build Status](https://travis-ci.org/danylaporte/fixed_size_lru_map.svg?branch=master)](https://travis-ci.org/danylaporte/fixed_size_lru_map)

A fixed size cache that keeps only the most recently used values.

## Documentation
[API Documentation](https://danylaporte.github.io/fixed_size_lru_map/fixed_size_lru_map)

## Example

```rust
use fixed_size_lru_map::FixedSizeLruMap;

fn main() {
    let map = FixedSizeLruMap::with_capacity(2);
    let a = *map.get_or_init("a", || 10);
    let b = *map.get_or_init("a", || 12);
    assert_eq!(10, a);
    assert_eq!(10, b);
    assert_eq!(1, map.len());
}
```

## License

Dual-licensed to be compatible with the Rust project.

Licensed under the Apache License, Version 2.0
[http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0) or the MIT license
[http://opensource.org/licenses/MIT](http://opensource.org/licenses/MIT), at your
option. This file may not be copied, modified, or distributed
except according to those terms.