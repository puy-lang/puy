program LendCopyDuringCall;

// Copying a record while one of its fields is lent to a `var` parameter is an `EAliasError`
// (types.md chapter 3): the copy would not see the value given back. Assigning the record and
// passing it by value are both copies; a nested record holding the lent field counts too.

type
  TInner = record
    N: Integer;
    Name: string;
  end;
  TRec = record
    In1: TInner;
    Items: array[0..1] of TInner;
  end;

var
  R, Y: TRec;

procedure Show(V: TRec);
begin
  WriteLn(V.In1.N);
end;

procedure Change(var X: Integer; var S: string);
begin
  X := 7;
  S := 'new';
  try
    Y := R;
  except
    on E: EAliasError do
      WriteLn('assign: ', E.Message);
  end;
  try
    Show(R);
  except
    on E: EAliasError do
      WriteLn('value parameter: ', E.Message);
  end;
  try
    Y.Items := R.Items;
  except
    on E: EAliasError do
      WriteLn('static array: ', E.Message);
  end;
  // A field that holds no lent location can be copied
  Y.In1 := R.Items[0];
end;

begin
  R := Default(TRec);
  R.In1.Name := 'old';
  R.Items[0].Name := 'zero';
  R.Items[1].Name := 'item';
  Change(R.In1.N, R.Items[1].Name);
  WriteLn(R.In1.N, ' ', R.Items[1].Name, ' ', Y.In1.N, ' ', Y.In1.Name);
  Y := R;
  WriteLn(Y.In1.N, ' ', Y.Items[1].Name);
end.
