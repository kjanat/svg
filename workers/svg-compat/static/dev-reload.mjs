const script = document.currentScript;
let boot = script instanceof HTMLScriptElement ? script.dataset.boot ?? '' : '';

setInterval(() => {
	fetch('/__reload')
		.then((response) => response.text())
		.then((nextBoot) => {
			if (nextBoot === boot) return;
			boot = nextBoot;
			location.reload();
		});
}, 500);
