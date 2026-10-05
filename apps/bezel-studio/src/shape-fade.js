/** Endpoints of a linear fade across an element box. */
export function fadeLine(frame, angle) {
  const a = angle * Math.PI / 180;
  const cos = Math.cos(a), sin = Math.sin(a);
  const reach = Math.abs(cos) + Math.abs(sin);
  // In normalized coordinates the gradient spans the projected box corners.
  const denominator = cos * cos / (frame.width * frame.width) + sin * sin / (frame.height * frame.height);
  const dx = cos / frame.width * reach / (2 * denominator);
  const dy = sin / frame.height * reach / (2 * denominator);
  const cx = frame.x + frame.width / 2, cy = frame.y + frame.height / 2;
  return [cx - dx, cy - dy, cx + dx, cy + dy];
}
