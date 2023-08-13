
/// 
/// Modified variant of 'copy_from_slice'.
/// 
/// Does not require equal length of both slices. It will only copy
/// only up to dst.len() or up to src.len().
/// 
/// In case src.len() < dst.len(), the remaining content of dst won't
/// be modified.
/// 
pub fn copy<T: Copy>(src: &[T], dst: &mut [T]) -> usize{
	let len = std::cmp::min(src.len(), dst.len());
	unsafe {
		std::ptr::copy_nonoverlapping(
			src.as_ptr(), 
			dst.as_mut_ptr(), 
			len
		)
	}
	len
}