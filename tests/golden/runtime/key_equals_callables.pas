program KeyEqualsCallables;

// Procedural values, method references and anonymous methods as keys (equality.md 2.7): the
// rules of `=`, with a hash; an anonymous method expression evaluated twice gives two different
// keys; a method reference to a destroyed instance is not equal to `nil` (unlike `=`).

type
  TProcP = procedure;
  TNotify = procedure of object;
  TAction = reference to procedure;

  TFoo = class
    procedure Run;
    procedure Stop;
  end;

procedure TFoo.Run;
begin
end;

procedure TFoo.Stop;
begin
end;

procedure One;
begin
end;

procedure Two;
begin
end;

function Make: TAction;
begin
  Result := procedure begin end;
end;

var
  P1, P2: TProcP;
  O1, O2: TFoo;
  M1, M2: TNotify;
  A1, A2: TAction;
begin
  P1 := One;
  P2 := One;
  WriteLn('procedural: ', KeyEquals(P1, P2), ' ', KeyEquals<TProcP>(P1, Two), ' ',
    KeyHash(P1) = KeyHash(P2));
  WriteLn('procedural nil: ', KeyEquals<TProcP>(nil, nil), ' ', KeyEquals<TProcP>(P1, nil));
  O1 := TFoo.Create;
  O2 := TFoo.Create;
  M1 := O1.Run;
  M2 := O1.Run;
  WriteLn('method reference: ', KeyEquals(M1, M2), ' ', KeyHash(M1) = KeyHash(M2));
  M2 := O1.Stop;
  WriteLn('other method: ', KeyEquals(M1, M2));
  M2 := O2.Run;
  WriteLn('other instance: ', KeyEquals(M1, M2));
  A1 := Make();
  A2 := A1;
  WriteLn('anonymous method, copied: ', KeyEquals(A1, A2), ' ', KeyHash(A1) = KeyHash(A2));
  A2 := Make();
  WriteLn('anonymous method, evaluated twice: ', KeyEquals(A1, A2));
  // `=` treats a method reference to a destroyed instance as `nil`; key equality does not
  M2 := O1.Run;
  O1.Free;
  WriteLn('destroyed: ', M1 = nil, ' ', KeyEquals<TNotify>(M1, nil), ' ', KeyEquals(M1, M2),
    ' ', KeyHash(M1) = KeyHash(M2));
  O2.Free;
end.
