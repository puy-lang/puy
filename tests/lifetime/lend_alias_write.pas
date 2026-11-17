program LendAliasWrite;

// A field passed to a `var` parameter is taken out of the record for the call and given back
// after it (types.md chapter 3, lifetime.md 3.7). The callee may write the other fields of the
// global `R`, but writing the lent field through `R` is an `EAliasError`. The values written
// through the parameters before the exception are still given back.

type
  TRec = record
    F, G: Integer;
    S: string;
  end;

var
  R: TRec;

procedure Touch(var X: Integer; var T: string);
begin
  R.G := 1;
  X := 5;
  T := T + '!';
  R.S := 'g';
  WriteLn('not reached');
end;

begin
  R := Default(TRec);
  R.S := 's';
  try
    Touch(R.F, R.S);
  except
    on E: EAliasError do
      WriteLn(E.Message);
  end;
  WriteLn(R.F, ' ', R.G, ' ', R.S);
end.
