// `uses` of a unit marked `experimental` is W0012 `unit `U2` is experimental` on the `uses` item
// (attributes.md 6.2); its routines are not reported again.
program Main;

uses U2;

begin
  Hello;
end.
