// puy-test: release
program StdlibHashSetChangedKeyRelease;

// In a release build only placing the items again (growing, TrimExcess) checks the remembered
// hashes (collections.md 8, 7.5): the changed item is not found by its old value nor by its new
// one, and enumerating and ToArray do not check.

uses System.SysUtils, System.Generics.Collections;

type
  TKey = class(TRefObject)
  public
    Name: string;
    constructor Create(const AName: string);
    function Equals(Obj: TObject): Boolean; override;
    function GetHashCode: Int64; override;
  end;

constructor TKey.Create(const AName: string);
begin
  inherited Create;
  Name := AName;
end;

function TKey.Equals(Obj: TObject): Boolean;
begin
  Result := (Obj is TKey) and (TKey(Obj).Name = Name);
end;

function TKey.GetHashCode: Int64;
begin
  Result := KeyHash(Name);
end;

var
  S: THashSet<TKey>;
  A, B, C: TKey;
  I: Integer;
  Items: TArray<TKey>;

procedure Report(E: Exception);
begin
  WriteLn('  ', E.ClassName, ': ', E.Message);
end;

procedure Find(const Name: string);
begin
  try
    WriteLn('  ', Name, ': ', S.Contains(TKey.Create(Name)));
  except
    on E: Exception do Report(E);
  end;
end;

procedure Enumerate;
var
  K: TKey;
begin
  try
    Write('  items:');
    for K in S do
      Write(' ', K.Name);
    WriteLn;
  except
    on E: Exception do
    begin
      WriteLn;
      Report(E);
    end;
  end;
end;

procedure Done(const Title: string; Changed: Boolean);
begin
  WriteLn('  ', Title, ': ', Changed, ', count ', S.Count);
end;

begin
  A := TKey.Create('a');
  B := TKey.Create('b');
  C := TKey.Create('c');
  S := THashSet<TKey>.Create([A, B, C]);

  WriteLn('b changed to x');
  B.Name := 'x';
  Find('b');
  Find('x');
  Find('a');
  Enumerate;
  try
    WriteLn('  ToArray: ', Length(S.ToArray));
  except
    on E: Exception do Report(E);
  end;

  WriteLn('removing');
  try
    Done('remove b', S.Remove(TKey.Create('b')));
  except
    on E: Exception do Report(E);
  end;
  try
    Done('remove c', S.Remove(TKey.Create('c')));
  except
    on E: Exception do Report(E);
  end;

  WriteLn('growing places the items again');
  for I := 1 to 3 do
    try
      Done('add n' + IntToStr(I), S.Add(TKey.Create('n' + IntToStr(I))));
    except
      on E: Exception do Report(E);
    end;
  try
    S.TrimExcess;
  except
    on E: Exception do Report(E);
  end;
  WriteLn('  count ', S.Count, ', capacity ', S.Capacity);

  WriteLn('Clear always works');
  S.Clear;
  WriteLn('  count ', S.Count);
  S.Add(B);
  Find('x');
  S.Free;

  // The set operations check every item before they change anything, so that a changed item
  // leaves the set as it was even when the change would come to light only part way, when the
  // items are placed again (collections.md 3.2, 8)
  WriteLn('set operations');
  A := TKey.Create('s0');
  S := THashSet<TKey>.Create([A, TKey.Create('s1')]);
  A.Name := 'changed';
  try
    S.UnionWith([TKey.Create('u1'), TKey.Create('u2'), TKey.Create('u3')]);
  except
    on E: Exception do Report(E);
  end;
  WriteLn('  count ', S.Count, ' after UnionWith');
  S.Clear;
  A.Name := 's0';
  S.Add(A);
  for I := 1 to 30 do
    S.Add(TKey.Create('s' + IntToStr(I)));
  A.Name := 'changed';
  Items := nil;
  for I := 1 to 20 do
    Items := Items + [TKey.Create('s' + IntToStr(I))];
  try
    S.ExceptWith(Items);
  except
    on E: Exception do Report(E);
  end;
  WriteLn('  count ', S.Count, ' after ExceptWith');
  try
    S.IntersectWith([TKey.Create('s30')]);
  except
    on E: Exception do Report(E);
  end;
  WriteLn('  count ', S.Count, ' after IntersectWith');
  S.Free;
end.
