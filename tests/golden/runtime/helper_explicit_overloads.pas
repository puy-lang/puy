program HelperExplicitOverloads;

// `H.M(…)` when a helper overloads an instance method and a class method of the same name
// (objects.md 10.5, 10.10): the form with the receiver first (`X, …`) and the form without it
// (`…`) both go through the choice of overloads, so either kind can be named; `X.M` and
// `TFoo.M` choose among them as usual.

type
  TFoo = class
  public
    N: Integer;
  end;

  TFooHelper = class helper for TFoo
    procedure Show; overload;
    class procedure Show(A: Integer); overload;
  end;

  TIntHelper = record helper for Integer
    function Plus(B: Integer): Integer; overload;
    class function Plus(const S: string): Integer; static; overload;
  end;

procedure TFooHelper.Show;
begin
  WriteLn('instance ', N);
end;

class procedure TFooHelper.Show(A: Integer);
begin
  WriteLn('class ', A, ' of ', ClassName);
end;

function TIntHelper.Plus(B: Integer): Integer;
begin
  Result := Self + B;
end;

class function TIntHelper.Plus(const S: string): Integer;
begin
  Result := Length(S);
end;

var
  F: TFoo;
  I: Integer;
begin
  F := TFoo.Create;
  F.N := 7;
  TFooHelper.Show(F);
  TFooHelper.Show(5);
  F.Show;
  F.Show(3);
  TFoo.Show(4);
  F.Free;
  I := 10;
  WriteLn(TIntHelper.Plus(I, 5), ' ', TIntHelper.Plus('abc'), ' ', I.Plus(1));
end.
