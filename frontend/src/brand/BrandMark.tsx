import { BRAND, SYMBOL_PATH } from './geometry'
import './brand.css'

/** The approved symbol; the name remains ordinary text, not a newly approved wordmark. */
export function BrandMark() {
  return <span className="brand-mark" role="img" aria-label="ALASHI NETWORK">
    <svg aria-hidden="true" viewBox="0 0 364 332" width="32" height="30"><path d={SYMBOL_PATH} fill={BRAND.lilac} fillRule="evenodd" /></svg>
    <span className="brand-name" aria-hidden="true">ALASHI<span>NETWORK</span></span>
  </span>
}
