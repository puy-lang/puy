program DumpHints;

// The `hir` dump shows the hint directives at the end of the line of a declaration
// (attributes.md 7): types, fields, `class var`s, methods, properties, globals, routines and
// local variables, without the description.

type
  TOld = Integer deprecated 'use Integer';
  TFoo = class
    F: Integer experimental;
    class var Count: Integer deprecated;
    procedure Run; deprecated;
    property P: Integer read F deprecated experimental;
  end deprecated;

var
  G: Integer = 1 experimental;

procedure TFoo.Run;
begin
end;

procedure Old; deprecated 'use New';
var
  L: Integer deprecated;
begin
  L := G;
  WriteLn(L);
end;

begin
  Old;
end.
