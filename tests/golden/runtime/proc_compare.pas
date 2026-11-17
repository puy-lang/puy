program ProcCompare;

// `=` and `<>` of procedural values compare the values without calling them (objects.md 9.5):
// procedural type values are equal for the same routine, also across two named types of the same
// signature; method references are equal for the same instance and method, and the method of a
// virtual method is the one the reference was made with; class method references compare the
// class and the method; `nil` on one side is `not Assigned`. A setter that skips an unchanged
// handler is written with `=`.

type
  TFunc = function(X: Integer): Integer;
  TOtherFunc = function(Y: Integer): Integer;
  TEvent = procedure of object;
  TClassFn = function: string of object;

  TThing = class
    procedure Ping; virtual;
    procedure Pong;
    class function Name: string; virtual;
  end;

  TSubThing = class(TThing)
    procedure Ping; override;
    class function Name: string; override;
  end;

  TClassOfThing = class of TThing;

  THolder = class
  private
    FOnPing: TEvent;
    procedure SetOnPing(Value: TEvent);
  public
    property OnPing: TEvent read FOnPing write SetOnPing;
  end;

procedure TThing.Ping;
begin
end;

procedure TThing.Pong;
begin
end;

class function TThing.Name: string;
begin
  Result := 'thing';
end;

procedure TSubThing.Ping;
begin
end;

class function TSubThing.Name: string;
begin
  Result := 'sub';
end;

procedure THolder.SetOnPing(Value: TEvent);
begin
  if FOnPing = Value then
  begin
    WriteLn('unchanged');
    Exit;
  end;
  FOnPing := Value;
  WriteLn('changed');
end;

function Inc1(X: Integer): Integer;
begin
  Result := X + 1;
end;

function Dec1(X: Integer): Integer;
begin
  Result := X - 1;
end;

var
  F, G: TFunc;
  O: TOtherFunc;
  A, B: TThing;
  E1, E2: TEvent;
  C: TClassOfThing;
  K1, K2: TClassFn;
  H: THolder;
begin
  F := Inc1;
  G := Inc1;
  O := Inc1;
  WriteLn(F = G, ' ', F <> G, ' ', F = Inc1, ' ', F = Dec1, ' ', F = O);
  G := Dec1;
  WriteLn(F = G, ' ', F <> nil, ' ', nil = F);
  A := TThing.Create;
  B := TSubThing.Create;
  E1 := A.Ping;
  E2 := A.Ping;
  WriteLn(E1 = E2, ' ', E1 = A.Pong, ' ', E1 = B.Ping);
  E2 := B.Ping;
  WriteLn(E1 <> E2, ' ', E2 = B.Ping);
  C := TSubThing;
  K1 := C.Name;
  K2 := TSubThing.Name;
  WriteLn(K1 = K2, ' ', K1 = TThing.Name);
  H := THolder.Create;
  H.OnPing := A.Ping;
  H.OnPing := A.Ping;
  H.OnPing := A.Pong;
  H.Free;
  E1 := nil;
  WriteLn(E1 = nil, ' ', E1 = E2);
  A.Free;
  B.Free;
end.
