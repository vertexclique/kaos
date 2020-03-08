<h1 align="center">
    <img src="https://github.com/vertexclique/kaos/raw/master/img/chaos.png"/>
</h1>
<div align="center">
 <strong>
   Chaotic Testing Harness
 </strong>
<hr>

[![Build Status](https://github.com/vertexclique/cuneiform/workflows/CI/badge.svg)](https://github.com/vertexclique/kaos/actions)
[![Latest Version](https://img.shields.io/crates/v/kaos.svg)](https://crates.io/crates/kaos)
[![Rust Documentation](https://img.shields.io/badge/api-rustdoc-blue.svg)](https://docs.rs/kaos/)
</div>

**Kaos** is a chaotic testing harness to test your services against random failures. It allows you to add points to your code to crash sporadically and harness asserts availability and fault tolerance of your services by seeking minimum time between failures, fail points, and randomized runs.

Kaos is equivalent of Chaos Monkey for the Rust ecosystem. But it is more smart to find the closest MTBF based on previous runs. This is dependable system practice. For more information please visit [Chaos engineering](https://en.wikipedia.org/wiki/Chaos_engineering).

<div align="center">
  [How to Use?](https://docs.rs/kaos/)
</div>
