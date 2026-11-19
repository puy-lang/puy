program ProcParenMember;

// A procedural value in parentheses, converted to a procedural type or kept in a record's field
// has no members (objects.md 9.10, 9.11), as a variable has: E0700 at the member's name, not the
// value called (E0725, E3200) or a missing field. A field's `Invoke` has the note `Rc.P()`; a
// value in parentheses or converted has no way to be called in place, and no note. A function
// value whose result is an object but has no such member is E0700 on the procedural type too.

type
  TAction = procedure;
  TRun = reference to procedure;
  TFn = reference to function: Integer;
  TObjFn = function: TObject;
  TRec = record
    P: TRun;
    A: TAction;
    F: TFn;
  end;

var
  A: TAction;
  V: TRun;
  F: TFn;
  Rc: TRec;
  X: Integer;
  G: TObjFn;
begin
  (A).Foo;
  (A).Invoke;
  ((A)).Invoke;
  (V).Invoke;
  (F).Invoke;
  TRun(V).Invoke;
  TAction(A).Foo;
  Rc.P.Invoke;
  Rc.A.Foo;
  Rc.F.Invoke;
  (Rc).P.Invoke;
  X := (F).Foo;
  X := Rc.F.Foo;
  X := F.Foo;
  G.Foo;
  X := (G).Foo;
end.
