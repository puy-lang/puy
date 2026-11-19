program Main;

// A generic type nested in a class of a unit in `uses` is named with type arguments through
// the class, qualified with the unit (`Shapes.TC.TG<Integer>`) or not (`TC.TG<Integer>`), in a
// type position and in an expression (`.Create`, `is`), and through a descendant of the class
// (`TD.TG<string>`, `TG<Integer>` in a method of `TD`; generics.md 2.5, units.md 3.1). Every
// way names the same instantiation as the one named through an alias of the class.

uses Shapes;

type
  TA = Shapes.TC;
  TD = class(TC)
    procedure Run;
  end;

procedure TD.Run;
var
  G: TG<Integer>;
begin
  G := TG<Integer>.Create;
  G.V := 7;
  WriteLn(G.V, ' ', G is TA.TG<Integer>);
  G.Free;
end;

var
  A: Shapes.TC.TG<Integer>;
  B: TC.TG<Integer>;
  C: TA.TG<Integer>;
  S: TD.TG<string>;
  O: TObject;
  D: TD;
begin
  A := Shapes.TC.TG<Integer>.Create;
  A.V := 1;
  B := TC.TG<Integer>.Create;
  B.V := 2;
  C := A;
  C.V := C.V + B.V;
  WriteLn(A.V);
  O := B;
  WriteLn(O is TC.TG<Integer>, ' ', O is Shapes.TC.TG<Integer>, ' ', O is TC.TG<string>);
  S := TD.TG<string>.Create;
  S.V := 'hi';
  O := S;
  WriteLn(S.V, ' ', O is TC.TG<string>, ' ', O is TD.TG<string>);
  D := TD.Create;
  D.Run;
  D.Free;
  A.Free;
  B.Free;
  S.Free;
end.
