/**
 * Runs before paint so the stored system/preference theme applies without a
 * flash. Shared by the root layout and the standalone `global-error` document,
 * which renders outside the root layout.
 */
export const THEME_SCRIPT = `(function(){try{var s=localStorage.getItem('theme');var light=s?s==='light':window.matchMedia('(prefers-color-scheme: light)').matches;var r=document.documentElement;r.classList.toggle('light',light);r.classList.toggle('dark',!light);r.style.colorScheme=light?'light':'dark';}catch(e){}})();`;
