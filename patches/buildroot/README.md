# Buildroot patches

Changes to Buildroot itself, for what a `BR2_EXTERNAL` tree can't change: a package's
`Config.in` and its dependencies. The top `Makefile` extracts Buildroot from its tarball
and applies these in order with `support/scripts/apply-patches.sh`, so they must apply
without fuzz. A new or changed patch extracts Buildroot again, then runs `make config`.
Each patch's header says why and whether it was sent upstream.

- `0001-mesa3d-panfrost-without-target-llvm.patch`: Panfrost's shaders are precompiled
  by host-mesa3d, so the device needs no LLVM, Clang, libclc or SPIR-V LLVM translator.
  Buildroot built them anyway, about 2 of the 5 hours of a build without ccache.
