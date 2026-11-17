program Main;

// The generic methods of a helper of a unit in `uses` (generics.md 2.1, 5.1; objects.md 10.4,
// 10.5): called through a value of the helped class with the type arguments written or inferred,
// in the form `H.M(X, …)` qualified with the unit's name, and as a `static` method.

uses Picks;

var
  C: TCounter;
  O: TObject;
begin
  C := TCounter.Create;
  WriteLn(C.Pick<string>('a', 'b'));
  WriteLn(C.Pick(1, 2));
  WriteLn(Picks.TCounterHelper.Pick(C, 'x', 'y'));
  WriteLn(TCounterHelper.Pick<Char>(C, 'p', 'q'));
  WriteLn(C.Count);
  O := TObject.Create;
  WriteLn(C.Same(O, O), ' ', C.Same<TObject>(O, C));
  O.Free;
  WriteLn(Length(TCounter.Repeated('z', 3)), ' ', Length(TCounterHelper.Repeated<Integer>(7, 2)));
  C.Free;
end.
