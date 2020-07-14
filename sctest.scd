s.waitForBoot({
	"booted".postln;
	{ Out.ar(0, SoundIn.ar([0,1]) *  SinOsc.ar([200,320])) }.play;
})


/*s.waitForBoot({
	"booted".postln;
	{ Out.ar(0, SinOsc.ar(400!2) * 0.5 ) }.play;
})*/


