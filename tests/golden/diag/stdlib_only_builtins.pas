// The built-ins only the units of the standard library use are not names for a user's program,
// even with the standard library units used, plain or qualified with `System`
// (docs/spec/collections.md 12.1).
program StdlibOnlyBuiltins;

uses System.Generics.Collections, System.Math, System.Character;

var
  A: TArray<Integer>;
  O: TObject;
  B: Boolean;
begin
  A := [1, 2, 3];
  ArrayMoveRange(A, 0, 1, 1);
  FreeIfClass(O);
  B := IsRefCountedObject(O);
  B := IsOverridden(B);
  System.ArrayMoveRange(A, 0, 1, 1);
  WriteLn(MathFunc1(0, 1.0));
  WriteLn(System.MathFunc2(1, 2.0, 3.0));
  WriteLn(UnicodeCategory(65));
  WriteLn(System.CodePointClass(65));
  WriteLn(NumericValue(65));
  RequestLeakReport;
  System.RequestLeakReport;
end.
