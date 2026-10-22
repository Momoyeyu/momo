"""Provider probes keep credentials out of browser networking and diagnostic output."""
import time
from urllib.parse import urlsplit, urlunsplit
import httpx
from fastapi import APIRouter, HTTPException
from pydantic import BaseModel

router = APIRouter()
class Probe(BaseModel):
    url: str
    key: str = ""

@router.post('/api/probe/llm')
async def llm_probe(cfg: Probe):
    try:
        parsed = urlsplit(cfg.url)
        if parsed.scheme not in ('http', 'https') or not parsed.hostname or parsed.username or parsed.password or parsed.fragment or parsed.query:
            raise ValueError()
        parsed.port
    except ValueError:
        raise HTTPException(400, 'Invalid provider URL')
    path = parsed.path.rstrip('/')
    if path.endswith('/chat/completions'):
        path = path[:-len('/chat/completions')]
    if not path.endswith('/v1'):
        path += '/v1'
    url = urlunsplit((parsed.scheme, parsed.netloc, path + '/models', '', ''))
    started = time.monotonic()
    try:
        async with httpx.AsyncClient(timeout=5, follow_redirects=False, trust_env=False) as client:
            response = await client.get(url, headers={'Authorization': f'Bearer {cfg.key}'} if cfg.key else {})
        ok = response.is_success or response.status_code == 405
        return {'ok': ok, 'detail': f'{round((time.monotonic()-started)*1000)}ms' if ok else f'HTTP {response.status_code}'}
    except httpx.HTTPError:
        return {'ok': False, 'detail': 'unreachable'}
