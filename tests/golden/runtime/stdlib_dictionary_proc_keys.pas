program StdlibDictionaryProcKeys;

// Procedural values can be keys (collections.md 3.1, equality.md 2.7): routines are equal when
// they are the same routine, method references when they call the same method of the same
// instance, and anonymous methods only when they are the same value.

uses System.SysUtils, System.Generics.Collections;

type
  TProcA = procedure;
  TCounter = class
  public
    N: Integer;
    procedure Bump;
  end;

procedure First;
begin
end;

procedure Second;
begin
end;

procedure TCounter.Bump;
begin
  Inc(N);
end;

var
  Routines: TDictionary<TProcA, string>;
  Methods: TDictionary<TProc, string>;
  C1, C2: TCounter;
  Anon: TProc;
  S: string;
begin
  Routines := TDictionary<TProcA, string>.Create;
  Routines.Add(First, 'first');
  Routines.Add(Second, 'second');
  Routines.Add(nil, 'nil');
  WriteLn(Routines[First], ' ', Routines[Second], ' ', Routines[nil], ' ', Routines.Count);
  try
    Routines.Add(First, 'again');
  except
    on E: EListError do WriteLn(E.Message);
  end;
  Routines.Free;

  C1 := TCounter.Create;
  C2 := TCounter.Create;
  Anon := procedure begin end;
  Methods := TDictionary<TProc, string>.Create;
  Methods.Add(C1.Bump, 'c1');
  Methods.Add(C2.Bump, 'c2');
  Methods.Add(Anon, 'anon');
  WriteLn(Methods[C1.Bump], ' ', Methods[C2.Bump], ' ', Methods[Anon]);
  WriteLn(Methods.TryGetValue(procedure begin end, S), ' ', Methods.Count);
  Methods.Free;
  C1.Free;
  C2.Free;
end.
