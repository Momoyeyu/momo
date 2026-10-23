// A rendered WebView element can still be covered by a scene or dialogue entry.
// Native functional tests wait for the same interaction gate as a real player.
export async function waitForRouteReady(route) {
  await browser.waitUntil(async()=>browser.execute(target=>{
    const shell=document.querySelector(target==='title'?'.title-screen':`.${target}-page`);
    const current=location.hash.replace(/^#\/?/, '')||'title';
    return current===target && !!shell && !document.querySelector('#startup') && !shell.closest('[inert]');
  },route),{timeout:30000,timeoutMsg:`Native ${route} did not become interactive`});
}
