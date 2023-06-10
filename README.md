# CSC 453 Lab 4 - TinyFS RS

## Demo

![Video Demo of TinyFS RS](video-demo.mp4)

## Names

Benjamin Hinchliff (bhinchli@calpoly.edu)

## How It Works

File breakdown:

- src/disk.rs - libDisk emulator as the `Disk` struct
- src/structures.rs - serialization structures for the filesystem
- src/lib.rs - main fs implementation and in-memory structures
- src/main.rs - filesystem demo application

The filesystem follows rust conventions for file handling, meaning that rather
than returning a file descriptor on open, it returns a file struct, from which
operations can then be performed. This file struct, along with the filesystem
struct itself, uses the Rust `Drop` trait to perform sync and cleanup operations
when they go out of scope, performing similar operations to `unmount` and
`fclose`

A two stage system is used for serializing the filesystem, the in-memory
structs, ones that use types that are easy to use from within rust, are first
converted to "serialization" structures listed in `structures.rs`, which use
types very close to the representation of the structs on disk (the only
difference being no standard byte ordering for integers and padding for
alignment). Then, those structures are finally serialized with `serde` and
`bincode`, Rust serialization libraries which standardize the representation.
This scheme allows for high level representation of filesystem structures in
memory while still retaining fairly easy serialization abilities, at the cost
of having two structs for each filesystem structure.

## Additional Functionality

All additional functionality is used and demonstrating working in the demo.

### Renaming and Listing

Renaming is supported by the `TfsFile.rename` method, where you pass in the
the name to rename the file to. The File object is not invalidated.
(see main.rs:100)

Listing is supported via the filesystem global `Tfs.readdir` which returns
an iterable (used to be an Iter but is now just a Vec for borrow checker
reasons with using interior mutability) of `DirEntry` containing the filename
and associated metadata.
(see main.rs:62)

### Timestamps

Supported via `TfsFile.stat`, also returns file size. Times are turned as
Rust `SystemTime` instances.
(see main.rs:62, used indirectly via the readdir method)

## Limitations

There are quite severe limitations on filesystem and file size.

- max filesystem size: 506 KiB
- max file size: 53.25 KiB
- max files: 128

The max filesystem size is 506 KiB due to the size of allocation table
implemented as a bitmask in the superblock.

The max filesize is due to the direct mapping used for the inodes, however
given the already small size of the filesystem I deemed the small size
acceptable.

The max files is due to the fact that the reference to each inode takes 2 bytes
and the root directory can therefore only contain 128 references.

If any of these limits should be exceeded the filesystem will throw an error
when it attempts to sync its in memory changes to disk.

## Bugs

There are no known bugs with the filesystem, but I'm sure there are some.
