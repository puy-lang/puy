program StdlibDictionaryChangedKey;

// In a debug build a dictionary also checks every entry a lookup compares and every entry it
// enumerates against the hash it remembered (collections.md 7.5), so the changed key is found by
// a lookup with its old value, by enumerating, and by growing.

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
  D: TDictionary<TKey, Integer>;
  A, B, C: TKey;
  Copied: TArray<TPair<TKey, Integer>>;
  I: Integer;

procedure Report(E: Exception);
begin
  WriteLn('  ', E.ClassName, ': ', E.Message);
end;

procedure Find(const Name: string);
var
  V: Integer;
begin
  try
    WriteLn('  ', Name, ': ', D.TryGetValue(TKey.Create(Name), V), ' ', V);
  except
    on E: Exception do Report(E);
  end;
end;

procedure Enumerate;
var
  P: TPair<TKey, Integer>;
begin
  try
    Write('  entries:');
    for P in D do
      Write(' ', P.Key.Name, '=', P.Value);
    WriteLn;
  except
    on E: Exception do
    begin
      WriteLn;
      Report(E);
    end;
  end;
end;

begin
  A := TKey.Create('a');
  B := TKey.Create('b');
  C := TKey.Create('c');
  D := TDictionary<TKey, Integer>.Create;
  D.Add(A, 1);
  D.Add(B, 2);
  D.Add(C, 3);

  WriteLn('b changed to x');
  B.Name := 'x';
  // The old value finds the entry by its hash but compares unequal; the new value has another hash
  Find('b');
  Find('x');
  // Other keys work
  Find('a');
  Find('c');
  Enumerate;

  WriteLn('removing');
  try
    D.Remove(TKey.Create('b'));
    WriteLn('  count ', D.Count);
  except
    on E: Exception do Report(E);
  end;

  WriteLn('growing places the entries again');
  for I := 1 to 3 do
    try
      D.Add(TKey.Create('n' + IntToStr(I)), 3 + I);
      WriteLn('  count ', D.Count, ', capacity ', D.Capacity);
    except
      on E: Exception do Report(E);
    end;
  try
    D.Capacity := 10;
  except
    on E: Exception do Report(E);
  end;
  WriteLn('  count ', D.Count, ', capacity ', D.Capacity);

  WriteLn('copying the entries to a new dictionary');
  try
    Copied := D.ToArray;
    D.Clear;
    for I := 0 to High(Copied) do
      D.Add(Copied[I].Key, Copied[I].Value);
    Enumerate;
    Find('x');
  except
    on E: Exception do Report(E);
  end;

  WriteLn('Clear always works');
  D.Clear;
  WriteLn('  count ', D.Count);
  D.Add(B, 2);
  Find('x');
  D.Free;

  // A removal that squeezes the holes out checks the keys before it removes anything: when it
  // raises, the entry is still there (collections.md 3.2)
  WriteLn('removing until the holes are squeezed out');
  D := TDictionary<TKey, Integer>.Create;
  A := TKey.Create('r0');
  D.Add(A, 0);
  for I := 1 to 30 do
    D.Add(TKey.Create('r' + IntToStr(I)), I);
  A.Name := 'changed';
  for I := 1 to 17 do
    try
      D.Remove(TKey.Create('r' + IntToStr(I)));
    except
      on E: Exception do
      begin
        Report(E);
        WriteLn('  count ', D.Count, ' after removing r', I);
      end;
    end;
  Find('r17');
  D.Free;
end.
