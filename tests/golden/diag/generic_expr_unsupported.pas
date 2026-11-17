program GenericExprUnsupported;

// A name written with type arguments in an expression (generics.md 1.4), wherever the expression
// is: a constant, a statement, a member of a value or a type, an argument, the right side of
// `is`. An undeclared name is E0700 (M4-11 resolves the names of generic types); a routine or a
// member written with type arguments that is not generic is E0761 (M4-14 resolves the generic
// routines and methods), over the name and its type arguments, and the left side of the member is
// still checked.

type
  TFoo = class
    procedure Run;
    function Get: Integer;
  end;

  TRec = record
    function Get: Integer;
  end;

procedure TFoo.Run;
begin
end;

function TFoo.Get: Integer;
begin
  Result := 1;
end;

function TRec.Get: Integer;
begin
  Result := 2;
end;

const
  K = Size<Integer>;

var
  O: TFoo;
  R: TRec;
  X: Integer;
  B: Boolean;

procedure Proc;
begin
end;

begin
  O := TFoo.Create;
  Proc<Integer>;
  O.Run<Integer>;
  X := O.Get<Integer>;
  X := R.Get<Integer>(1);
  X := TFoo.Get<Integer>;
  WriteLn(Max<Integer>(1, 2):3);
  var Y := TList<Integer>.Create;
  Y.Free;
  B := O is TBar<Integer>;
  X := SizeOf(TList<Integer>);
  X := System.Max<Integer>(1, 2);
  X := A<B>.C<D>.E;
  X := Undecl(Foo).Get<Integer>;
  raise EFoo<Integer>.Create('x');
end.
