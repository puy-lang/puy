program StdlibDictionaryErrors;

// The exceptions of a dictionary and their messages (collections.md 3.2, 3.3): a duplicate key
// is EListError, a missing key EKeyNotFound (an EListError too), a change during enumeration
// EInvalidOperation, a capacity out of range ERangeError. A member that raises leaves the
// dictionary as it was. Writing only the value of a key during enumeration is not a change.

uses System.SysUtils, System.Generics.Collections;

var
  D: TDictionary<string, Integer>;
  P: TPair<string, Integer>;
  K: string;
  V: Integer;

procedure Report(E: Exception);
begin
  WriteLn(E.ClassName, ': ', E.Message);
end;

begin
  D := TDictionary<string, Integer>.Create;
  D.Add('a', 1);
  D.Add('b', 2);

  try
    D.Add('a', 10);
  except
    on E: Exception do Report(E);
  end;
  WriteLn('a = ', D['a'], ', count ', D.Count);

  try
    V := D['x'];
  except
    on E: EListError do Report(E);
  end;
  try
    WriteLn(D['y']);
  except
    on E: EKeyNotFound do Report(E);
  end;

  try
    for P in D do
      if P.Key = 'a' then
        D.Add('c', 3);
  except
    on E: Exception do Report(E);
  end;
  WriteLn('count ', D.Count);

  // The change in the last iteration is caught by the MoveNext after it
  try
    for K in D.Keys do
      if K = 'c' then
        D.Remove('a');
  except
    on E: Exception do Report(E);
  end;
  try
    for V in D.Values do
      D.Clear;
  except
    on E: Exception do Report(E);
  end;

  D.Add('a', 1);
  D.Add('b', 2);
  for P in D do
  begin
    D[P.Key] := P.Value * 10;
    D.AddOrSetValue(P.Key, D[P.Key] + 1);
  end;
  for P in D do
    WriteLn(P.Key, '=', P.Value);

  try
    D.Capacity := 1;
  except
    on E: Exception do Report(E);
  end;
  D.Free;

  try
    D := TDictionary<string, Integer>.Create(-1);
  except
    on E: Exception do Report(E);
  end;

  try
    D := TDictionary<string, Integer>.Create([TPair<string, Integer>.Create('x', 1),
      TPair<string, Integer>.Create('x', 2)]);
  except
    on E: Exception do Report(E);
  end;

  D := TDictionary<string, Integer>.Create(8);
  WriteLn('capacity ', D.Capacity, ', count ', D.Count);
  D.Free;
end.
