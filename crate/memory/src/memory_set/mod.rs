//! memory set, area
//! and the inactive page table

use alloc::{boxed::Box, vec::Vec, string::String};
use core::fmt::{Debug, Error, Formatter};

use crate::paging::*;

use super::*;

use self::handler::MemoryHandler;

pub mod handler;

/// a continuous memory space when the same attribute
/// like `vma_struct` in ucore
#[derive(Debug, Clone)]
pub struct MemoryArea {
    start_addr: VirtAddr,
    end_addr: VirtAddr,
    handler: Box<MemoryHandler>,
    name: &'static str,
}

impl MemoryArea {
    /*
    **  @brief  get slice of the content in the memory area
    **  @retval &[u8]                the slice of the content in the memory area
    */
    pub unsafe fn as_slice(&self) -> &[u8] {
        ::core::slice::from_raw_parts(self.start_addr as *const u8, self.end_addr - self.start_addr)
    }
    /*
    **  @brief  get mutable slice of the content in the memory area
    **  @retval &mut[u8]             the mutable slice of the content in the memory area
    */
    pub unsafe fn as_slice_mut(&self) -> &mut [u8] {
        ::core::slice::from_raw_parts_mut(self.start_addr as *mut u8, self.end_addr - self.start_addr)
    }
    /*
    **  @brief  test whether a virtual address is in the memory area
    **  @param  addr: VirtAddr       the virtual address to test
    **  @retval bool                 whether the virtual address is in the memory area
    */
    pub fn contains(&self, addr: VirtAddr) -> bool {
        addr >= self.start_addr && addr < self.end_addr
    }
    /// Check the pointer is within the readable memory
    pub fn check_ptr<T>(&self, ptr: *const T) -> bool {
        self.check_array(ptr, 1)
    }
    /// Check the pointer is within the writable memory
    pub fn check_mut_ptr<T>(&self, ptr: *mut T) -> bool {
        self.check_mut_array(ptr, 1)
    }
    /// Check the array is within the readable memory
    pub fn check_array<S>(&self, ptr: *const S, count: usize) -> bool {
        // FIXME: check readable
        ptr as usize >= self.start_addr &&
            unsafe { ptr.offset(count as isize) as usize } <= self.end_addr
    }
    /// Check the array is within the writable memory
    pub fn check_mut_array<S>(&self, ptr: *mut S, count: usize) -> bool {
        // FIXME: check writable
        ptr as usize >= self.start_addr &&
            unsafe { ptr.offset(count as isize) as usize } <= self.end_addr
    }
    /// Check the null-end C string is within the readable memory, and is valid.
    /// If so, clone it to a String.
    ///
    /// Unsafe: the page table must be active.
    pub unsafe fn check_and_clone_cstr(&self, ptr: *const u8) -> Option<String> {
        if ptr as usize >= self.end_addr {
            return None;
        }
        let max_len = self.end_addr - ptr as usize;
        (0..max_len)
            .find(|&i| ptr.offset(i as isize).read() == 0)
            .and_then(|len| core::str::from_utf8(core::slice::from_raw_parts(ptr, len)).ok())
            .map(|s| String::from(s))
    }
    /// Test whether this area is (page) overlap with area [`start_addr`, `end_addr`]
    fn is_overlap_with(&self, start_addr: VirtAddr, end_addr: VirtAddr) -> bool {
        let p0 = Page::of_addr(self.start_addr);
        let p1 = Page::of_addr(self.end_addr - 1) + 1;
        let p2 = Page::of_addr(start_addr);
        let p3 = Page::of_addr(end_addr - 1) + 1;
        !(p1 <= p2 || p0 >= p3)
    }
    /*
    **  @brief  map the memory area to the physice address in a page table
    **  @param  pt: &mut T::Active   the page table to use
    **  @retval none
    */
    fn map(&self, pt: &mut PageTable) {
        for page in Page::range_of(self.start_addr, self.end_addr) {
            self.handler.map(pt, page.start_address());
        }
    }
    /*
    **  @brief  unmap the memory area from the physice address in a page table
    **  @param  pt: &mut T::Active   the page table to use
    **  @retval none
    */
    fn unmap(&self, pt: &mut PageTable) {
        for page in Page::range_of(self.start_addr, self.end_addr) {
            self.handler.unmap(pt, page.start_address());
        }
    }
}

/// The attributes of the memory
#[derive(Debug, Copy, Clone, Eq, PartialEq, Default)]
pub struct MemoryAttr {
    user: bool,
    readonly: bool,
    execute: bool,
    mmio: u8,
}

impl MemoryAttr {
    /*
    **  @brief  set the memory attribute's user bit
    **  @retval MemoryAttr           the memory attribute itself
    */
    pub fn user(mut self) -> Self {
        self.user = true;
        self
    }
    /*
    **  @brief  set the memory attribute's readonly bit
    **  @retval MemoryAttr           the memory attribute itself
    */
    pub fn readonly(mut self) -> Self {
        self.readonly = true;
        self
    }
    /*
    **  @brief  set the memory attribute's execute bit
    **  @retval MemoryAttr           the memory attribute itself
    */
    pub fn execute(mut self) -> Self {
        self.execute = true;
        self
    }
    /*
    **  @brief  set the MMIO type
    **  @retval MemoryAttr           the memory attribute itself
    */
    pub fn mmio(mut self, value: u8) -> Self {
        self.mmio = value;
        self
    }
    /*
    **  @brief  apply the memory attribute to a page table entry
    **  @param  entry: &mut impl Entry
    **                               the page table entry to apply the attribute
    **  @retval none
    */
    pub fn apply(&self, entry: &mut Entry) {
        entry.set_present(true);
        entry.set_user(self.user);
        entry.set_writable(!self.readonly);
        entry.set_execute(self.execute);
        entry.set_mmio(self.mmio);
        entry.update();
    }
}

/// set of memory space with multiple memory area with associated page table and stack space
/// like `mm_struct` in ucore
pub struct MemorySet<T: InactivePageTable> {
    areas: Vec<MemoryArea>,
    page_table: T,
}

impl<T: InactivePageTable> MemorySet<T> {
    /*
    **  @brief  create a memory set
    **  @retval MemorySet<T>         the memory set created
    */
    pub fn new() -> Self {
        MemorySet {
            areas: Vec::new(),
            page_table: T::new(),
        }
    }
    pub fn new_bare() -> Self {
        MemorySet {
            areas: Vec::new(),
            page_table: T::new_bare(),
        }
    }
    /// Check the pointer is within the readable memory
    pub fn check_ptr<S>(&self, ptr: *const S) -> bool {
        self.areas.iter().find(|area| area.check_ptr(ptr)).is_some()
    }
    /// Check the pointer is within the writable memory
    pub fn check_mut_ptr<S>(&self, ptr: *mut S) -> bool {
        self.areas.iter().find(|area| area.check_mut_ptr(ptr)).is_some()
    }
    /// Check the array is within the readable memory
    pub fn check_array<S>(&self, ptr: *const S, count: usize) -> bool {
        self.areas.iter().find(|area| area.check_array(ptr, count)).is_some()
    }
    /// Check the array is within the writable memory
    pub fn check_mut_array<S>(&self, ptr: *mut S, count: usize) -> bool {
        self.areas.iter().find(|area| area.check_mut_array(ptr, count)).is_some()
    }
    /// Check the null-end C string is within the readable memory, and is valid.
    /// If so, clone it to a String.
    ///
    /// Unsafe: the page table must be active.
    pub unsafe fn check_and_clone_cstr(&self, ptr: *const u8) -> Option<String> {
        self.areas.iter()
            .filter_map(|area| area.check_and_clone_cstr(ptr))
            .next()
    }
    /// Find a free area with hint address `addr_hint` and length `len`.
    /// Return the start address of found free area.
    /// Used for mmap.
    pub fn find_free_area(&self, addr_hint: usize, len: usize) -> VirtAddr {
        // brute force:
        // try each area's end address as the start
        core::iter::once(addr_hint)
            .chain(self.areas.iter().map(|area| area.end_addr))
            .map(|addr| addr + PAGE_SIZE) // move up a page
            .find(|&addr| self.test_free_area(addr, addr + len))
            .expect("failed to find free area ???")
    }
    fn test_free_area(&self, start_addr: usize, end_addr: usize) -> bool {
        self.areas.iter()
            .find(|area| area.is_overlap_with(start_addr, end_addr))
            .is_none()
    }
    /*
    **  @brief  add the memory area to the memory set
    **  @param  area: MemoryArea     the memory area to add
    **  @retval none
    */
    pub fn push(&mut self, start_addr: VirtAddr, end_addr: VirtAddr, handler: impl MemoryHandler, name: &'static str) {
        assert!(start_addr <= end_addr, "invalid memory area");
        assert!(self.test_free_area(start_addr, end_addr), "memory area overlap");
        let area = MemoryArea { start_addr, end_addr, handler: Box::new(handler), name };
        self.page_table.edit(|pt| area.map(pt));
        self.areas.push(area);
    }
    /*
    **  @brief  get iterator of the memory area
    **  @retval impl Iterator<Item=&MemoryArea>
    **                               the memory area iterator
    */
    pub fn iter(&self) -> impl Iterator<Item=&MemoryArea> {
        self.areas.iter()
    }
    pub fn edit(&mut self, f: impl FnOnce(&mut T::Active)) {
        self.page_table.edit(f);
    }
    /*
    **  @brief  execute function with the associated page table
    **  @param  f: impl FnOnce()     the function to be executed
    **  @retval none
    */
    pub unsafe fn with(&self, f: impl FnOnce()) {
        self.page_table.with(f);
    }
    /*
    **  @brief  activate the associated page table
    **  @retval none
    */
    pub unsafe fn activate(&self) {
        self.page_table.activate();
    }
    /*
    **  @brief  get the token of the associated page table
    **  @retval usize                the token of the inactive page table
    */
    pub fn token(&self) -> usize {
        self.page_table.token()
    }
    /*
    **  @brief  clear the memory set
    **  @retval none
    */
    pub fn clear(&mut self) {
        let Self { ref mut page_table, ref mut areas, .. } = self;
        page_table.edit(|pt| {
            for area in areas.iter() {
                area.unmap(pt);
            }
        });
        areas.clear();
    }

    /*
    **  @brief  get the mutable reference for the inactive page table
    **  @retval: &mut T                 the mutable reference of the inactive page table
    */
    pub fn get_page_table_mut(&mut self) -> &mut T{
        &mut self.page_table
    }

    pub fn page_fault_handler(&mut self, addr: VirtAddr) -> bool {
        let area = self.areas.iter().find(|area| area.contains(addr));
        match area {
            Some(area) => self.page_table.edit(|pt| area.handler.page_fault_handler(pt, addr)),
            None => false,
        }
    }
}

impl<T: InactivePageTable> Clone for MemorySet<T> {
    fn clone(&self) -> Self {
        let mut page_table = T::new();
        page_table.edit(|pt| {
            for area in self.areas.iter() {
                area.map(pt);
            }
        });
        MemorySet {
            areas: self.areas.clone(),
            page_table,
        }
    }
}

impl<T: InactivePageTable> Drop for MemorySet<T> {
    fn drop(&mut self) {
        self.clear();
    }
}

impl<T: InactivePageTable> Debug for MemorySet<T> {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        f.debug_list()
            .entries(self.areas.iter())
            .finish()
    }
}
