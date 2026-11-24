program AssertValuesRules;

// Which subexpressions a failed Assert records and how (docs/spec/semantics.md 5.4): the right
// side of `and` / `or` not evaluated, the steps of a fold, a text recorded once, the implicit
// `Self`, a variable written later in the condition, the record a field is read from, a record's
// private fields, a destroyed instance, and the exceptions that record nothing.

type
  TSecret = record
  private
    FHidden: Integer;
  public
    Shown: Integer;
  end;
  TCounter = class
    FCount: Integer;
    function Count: Integer;
    procedure Check;
  end;
  TPlain = class
  end;
  EMine = class(EAssertionFailed);

function TCounter.Count: Integer;
begin
  Result := FCount * 2;
end;

procedure TCounter.Check;
begin
  Assert(Count + FCount = 0);
end;

procedure Show(const E: EAssertionFailed);
begin
  WriteLn(E.Message, ' (', Length(E.Values), ')');
  for var V in E.Values do
    WriteLn('  ', V.Expr, ' = ', V.Value);
end;

function Twin(const S: TSecret): TSecret;
begin
  Result := S;
end;

function Bump(var N: Integer): Integer;
begin
  Inc(N);
  Result := N;
end;

var
  A, B, N: Integer;
  Secret: TSecret;
  Counter: TCounter;
  Plain: TPlain;
begin
  A := 1;
  B := 2;
  try
    Assert((A = 2) and (B = 3));
  except
    on E: EAssertionFailed do Show(E);
  end;
  try
    Assert((A = 1) and ((B = 3) or (A + B = 4)));
  except
    on E: EAssertionFailed do Show(E);
  end;
  try
    Assert(A + B * 2 - A = 0, 'with a message');
  except
    on E: EAssertionFailed do Show(E);
  end;
  try
    Assert((A + 1 = 5) or (A + 1 = 6));
  except
    on E: EAssertionFailed do Show(E);
  end;
  N := 10;
  try
    Assert(N + Bump(N) = 0);
  except
    on E: EAssertionFailed do Show(E);
  end;
  Secret := Default(TSecret);
  Secret.Shown := 5;
  try
    Assert(Secret.Shown = 0);
  except
    on E: EAssertionFailed do Show(E);
  end;
  try
    Assert(Twin(Secret).Shown = 0);
  except
    on E: EAssertionFailed do Show(E);
  end;
  Counter := TCounter.Create;
  Counter.FCount := 3;
  try
    Counter.Check;
  except
    on E: EAssertionFailed do Show(E);
  end;
  Counter.Free;
  Plain := TPlain.Create;
  Plain.Free;
  try
    Assert(Plain = nil);
  except
    on E: EAssertionFailed do Show(E);
  end;
  try
    raise EAssertionFailed.Create('made by hand');
  except
    on E: EAssertionFailed do Show(E);
  end;
  try
    raise EMine.Create('a descendant');
  except
    on E: EAssertionFailed do Show(E);
  end;
  {$C-}
  Assert(A = 0);
  {$C+}
  WriteLn('done');
end.
