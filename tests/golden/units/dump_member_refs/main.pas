program Main;

// The hir dump numbers the functions it only refers to (declared in a unit the program uses) in
// the order of their first reference in the dump (semantics.md 9): the constructor of the
// field's attribute comes before the `read` method of the property declared after the field.

uses Marks;

type
  TFoo = class(TBase)
  public
    [Mark(1)] FX: Integer;
    property N: Integer read GetN;
  end;

var
  F: TFoo;

begin
  F := TFoo.Create;
  WriteLn(F.N);
  F.Free;
end.
