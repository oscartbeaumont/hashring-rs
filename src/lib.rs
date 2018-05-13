// MIT License

// Copyright (c) 2016 Jerome Froelich

// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:

// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.

// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

//! A minimal implementation of consistent hashing as described in [Consistent
//! Hashing and Random Trees: Distributed Caching Protocols for Relieving Hot
//! Spots on the World Wide Web] (https://www.akamai.com/es/es/multimedia/documents/technical-publication/consistent-hashing-and-random-trees-distributed-caching-protocols-for-relieving-hot-spots-on-the-world-wide-web-technical-publication.pdf).
//! Clients can use the `HashRing` struct to add consistent hashing to their
//! applications. `HashRing`'s API consists of three methods: `add`, `remove`,
//! and `get` for adding a node to the ring, removing a node from the ring, and
//! getting the node responsible for the provided key.
//!
//! ## Example
//!
//! Below is a simple example of how an application might use `HashRing` to make
//! use of consistent hashing. Since `HashRing` exposes only a minimal API clients
//! can build other abstractions, such as virtual nodes, on top of it. The example
//! below shows one potential implementation of virtual nodes on top of `HashRing`
//!
//! ``` rust,no_run
//! extern crate hashring;
//!
//! use std::net::{IpAddr, SocketAddr};
//! use std::str::FromStr;
//!
//! use hashring::HashRing;
//!
//! #[derive(Debug, Copy, Clone)]
//! struct VNode {
//!     id: usize,
//!     addr: SocketAddr,
//! }
//!
//! impl VNode {
//!     fn new(ip: &str, port: u16, id: usize) -> Self {
//!         let addr = SocketAddr::new(IpAddr::from_str(&ip).unwrap(), port);
//!         VNode {
//!             id: id,
//!             addr: addr,
//!         }
//!     }
//! }
//!
//! impl ToString for VNode {
//!     fn to_string(&self) -> String {
//!         format!("{}|{}", self.addr, self.id)
//!     }
//! }
//!
//! impl PartialEq for VNode {
//!     fn eq(&self, other: &VNode) -> bool {
//!         self.id == other.id && self.addr == other.addr
//!     }
//! }
//!
//! fn main() {
//!     let mut ring: HashRing<VNode, &str> = HashRing::new();
//!
//!     let mut nodes = vec![];
//!     nodes.push(VNode::new("127.0.0.1", 1024, 1));
//!     nodes.push(VNode::new("127.0.0.1", 1024, 2));
//!     nodes.push(VNode::new("127.0.0.2", 1024, 1));
//!     nodes.push(VNode::new("127.0.0.2", 1024, 2));
//!     nodes.push(VNode::new("127.0.0.2", 1024, 3));
//!     nodes.push(VNode::new("127.0.0.3", 1024, 1));
//!
//!     for node in nodes {
//!         ring.add(node);
//!     }
//!
//!     println!("{:?}", ring.get(&"foo"));
//!     println!("{:?}", ring.get(&"bar"));
//!     println!("{:?}", ring.get(&"baz"));
//! }
//! ```

extern crate crypto;

use std::cmp::Ordering;
use std::marker::PhantomData;

use crypto::digest::Digest;
use crypto::md5::Md5;

// Node is an internal struct used to encapsulate the nodes that will be added and
// removed from `HashRing`
#[derive(Debug)]
struct Node<T> {
    key: u64,
    node: T,
}

impl<T> Node<T> {
    fn new(key: u64, node: T) -> Node<T> {
        Node { key, node }
    }
}

// Implement `PartialEq`, `Eq`, `PartialOrd` and `Ord` so we can sort `Node`s
impl<T> PartialEq for Node<T> {
    fn eq(&self, other: &Node<T>) -> bool {
        self.key == other.key
    }
}

impl<T> Eq for Node<T> {}

impl<T> PartialOrd for Node<T> {
    fn partial_cmp(&self, other: &Node<T>) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for Node<T> {
    fn cmp(&self, other: &Node<T>) -> Ordering {
        self.key.cmp(&other.key)
    }
}

pub struct HashRing<T, U> {
    ring: Vec<Node<T>>,
    hash: Md5,
    buf: [u8; 16],
    phantom: PhantomData<U>,
}

/// Hash Ring
///
/// A hash ring that provides consistent hashing for nodes that are added to it.
impl<T, U> HashRing<T, U>
where
    T: ToString,
    U: ToString,
{
    /// Create a new HashRing.
    pub fn new() -> HashRing<T, U> {
        Default::default()
    }

    /// Get the number of nodes in the hash ring.
    pub fn len(&self) -> usize {
        self.ring.len()
    }

    /// Returns true if the ring has no elements.
    pub fn is_empty(&self) -> bool {
        self.ring.len() == 0
    }

    /// Add `node` to the hash ring.
           pub fn add(&mut self, node: T) {
        let s = node.to_string();
        let key = self.get_key(&s);
        self.ring.push(Node::new(key, node));
        self.ring.sort();
    }

    /// Remove `node` from the hash ring. Returns an `Option` that will contain the `node`
    /// if it was in the hash ring or `None` if it was not present.
    pub fn remove(&mut self, node: &T) -> Option<T> {
        let s = node.to_string();
        let key = self.get_key(&s);
        match self.ring.binary_search_by(|node| node.key.cmp(&key)) {
            Err(_) => None,
            Ok(n) => Some(self.ring.remove(n).node),
        }
    }

    /// Get the node responsible for `key`. Returns an `Option` that will contain the `node`
    /// if the hash ring is not empty or `None` if it was empty.
    pub fn get(&mut self, key: &U) -> Option<&T> {
        if self.ring.is_empty() {
            return None;
        }

        let s = key.to_string();
        let k = self.get_key(&s);

        let n = match self.ring.binary_search_by(|node| node.key.cmp(&k)) {
            Err(n) => n,
            Ok(n) => n,
        };

        if n == self.ring.len() {
            return Some(&self.ring[0].node);
        }

        Some(&self.ring[n].node)
    }

    // An internal function for converting a reference to a `str` into a `u64` which
    // can be used as a key in the hash ring.
    fn get_key(&mut self, s: &str) -> u64 {
        self.hash.reset();
        self.hash.input_str(s);
        self.hash.result(&mut self.buf);

        let n: u64 = u64::from(self.buf[7]) << 56 | u64::from(self.buf[6]) << 48
            | u64::from(self.buf[5]) << 40 | u64::from(self.buf[4]) << 32
            | u64::from(self.buf[3]) << 24 | u64::from(self.buf[2]) << 16
            | u64::from(self.buf[1]) << 8 | u64::from(self.buf[0]) as u64;

        n
    }
}

impl<T, U> Default for HashRing<T, U> {
    fn default() -> Self {
        HashRing {
            ring: Vec::new(),
            hash: Md5::new(),
            buf: [0; 16],
            phantom: PhantomData,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, SocketAddr};
    use std::str::FromStr;

    use super::HashRing;

    #[derive(Debug, Copy, Clone)]
    struct VNode {
        id: usize,
        addr: SocketAddr,
    }

    impl VNode {
        fn new(ip: &str, port: u16, id: usize) -> Self {
            let addr = SocketAddr::new(IpAddr::from_str(&ip).unwrap(), port);
            VNode { id: id, addr: addr }
        }
    }

    impl ToString for VNode {
        fn to_string(&self) -> String {
            format!("{}|{}", self.addr, self.id)
        }
    }

    impl PartialEq for VNode {
        fn eq(&self, other: &VNode) -> bool {
            self.id == other.id && self.addr == other.addr
        }
    }

    #[test]
    fn add_and_remove_nodes() {
        let mut ring: HashRing<VNode, &str> = HashRing::new();

        assert_eq!(ring.len(), 0);
        assert!(ring.is_empty());

        let vnode1 = VNode::new("127.0.0.1", 1024, 1);
        let vnode2 = VNode::new("127.0.0.1", 1024, 2);
        let vnode3 = VNode::new("127.0.0.2", 1024, 1);

        ring.add(vnode1);
        ring.add(vnode2);
        ring.add(vnode3);
        assert_eq!(ring.len(), 3);
        assert!(!ring.is_empty());

        assert_eq!(ring.remove(&vnode2).unwrap(), vnode2);
        assert_eq!(ring.len(), 2);

        let vnode4 = VNode::new("127.0.0.2", 1024, 2);
        let vnode5 = VNode::new("127.0.0.2", 1024, 3);
        let vnode6 = VNode::new("127.0.0.3", 1024, 1);

        ring.add(vnode4);
        ring.add(vnode5);
        ring.add(vnode6);

        assert_eq!(ring.remove(&vnode1).unwrap(), vnode1);
        assert_eq!(ring.remove(&vnode3).unwrap(), vnode3);
        assert_eq!(ring.remove(&vnode6).unwrap(), vnode6);
        assert_eq!(ring.len(), 2);
    }

    #[test]
    fn get_nodes() {
        let mut ring: HashRing<VNode, &str> = HashRing::new();

        assert_eq!(ring.get(&"foo"), None);

        let vnode1 = VNode::new("127.0.0.1", 1024, 1);
        let vnode2 = VNode::new("127.0.0.1", 1024, 2);
        let vnode3 = VNode::new("127.0.0.2", 1024, 1);
        let vnode4 = VNode::new("127.0.0.2", 1024, 2);
        let vnode5 = VNode::new("127.0.0.2", 1024, 3);
        let vnode6 = VNode::new("127.0.0.3", 1024, 1);

        ring.add(vnode1);
        ring.add(vnode2);
        ring.add(vnode3);
        ring.add(vnode4);
        ring.add(vnode5);
        ring.add(vnode6);

        assert_eq!(ring.get(&"foo"), Some(&vnode1));
        assert_eq!(ring.get(&"bar"), Some(&vnode2));
        assert_eq!(ring.get(&"baz"), Some(&vnode1));

        assert_eq!(ring.get(&"abc"), Some(&vnode6));
        assert_eq!(ring.get(&"def"), Some(&vnode3));
        assert_eq!(ring.get(&"ghi"), Some(&vnode3));

        assert_eq!(ring.get(&"cat"), Some(&vnode5));
        assert_eq!(ring.get(&"dog"), Some(&vnode6));
        assert_eq!(ring.get(&"bird"), Some(&vnode2));
    }
}
