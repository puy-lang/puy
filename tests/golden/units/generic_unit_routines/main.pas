program Main;

// A generic routine of a unit is instantiated by the program that writes its type arguments,
// qualified with the unit's name or not (generics.md 1.4, 4.1), and the overloads with the same
// number of type parameters are chosen by the arguments either way (5.4). The same calls
// without type arguments infer them (5.2).

uses Utils;

var
  O: TObject;
begin
  WriteLn(Max<Integer>(1, 2, True));
  WriteLn(Utils.Max<string>('a', 'b', False));
  WriteLn(TUtil.First<Char>(['x', 'y']));
  O := TObject.Create;
  WriteLn(Pick<TObject>(O) = O);
  O.Free;
  WriteLn(Plus<Integer>(1), ' ', Plus<Integer>(1, 2));
  WriteLn(Utils.Plus<string>('a'), ' ', Utils.Plus<string>('a', 'b'));
  WriteLn(Max(1, 2, True));
  WriteLn(Utils.Max('a', 'b', False));
  WriteLn(TUtil.First(['x', 'y']));
  O := TObject.Create;
  WriteLn(Pick(O) = O);
  O.Free;
  WriteLn(Plus(1), ' ', Plus(1, 2));
  WriteLn(Utils.Plus('a'), ' ', Utils.Plus('a', 'b'));
end.
