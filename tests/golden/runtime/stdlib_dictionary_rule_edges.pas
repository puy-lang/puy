program StdlibDictionaryRuleEdges;

// Rules of TDictionary<K, V> and TObjectDictionary<K, V> not seen elsewhere (collections.md 7.1,
// 9.3, 9.5): Create from an array of pairs and from another dictionary adds the pairs in order;
// ContainsValue uses key equality (a NaN value is found); the owning dictionary's constructors
// taking a comparer work at run time; a subclass's inherited Create with a constant set that owns
// a non-class side raises EInvalidOperation when the instance is created (no E0768, since it is
// not a creation expression); the half-made instance is destroyed as after any raising
// constructor, and the variable is not assigned.

uses System.SysUtils, System.Generics.Defaults, System.Generics.Collections;

type
  TItem = class
    Name: string;
    constructor Create(const AName: string);
    destructor Destroy; override;
  end;

  TOwnStringKeys = class(TObjectDictionary<string, TItem>)
  public
    constructor Create;
    destructor Destroy; override;
  end;

constructor TItem.Create(const AName: string);
begin
  Name := AName;
end;

destructor TItem.Destroy;
begin
  WriteLn('  free ', Name);
  inherited;
end;

constructor TOwnStringKeys.Create;
begin
  WriteLn('  TOwnStringKeys.Create');
  inherited Create([TDictionaryOwnership.doOwnsKeys]);
  WriteLn('  not reached');
end;

destructor TOwnStringKeys.Destroy;
begin
  WriteLn('  TOwnStringKeys.Destroy');
  inherited;
end;

var
  D, D2: TDictionary<string, Integer>;
  F: TDictionary<string, Double>;
  P: TPair<string, Integer>;
  OD: TObjectDictionary<string, TItem>;
  Sub: TOwnStringKeys;
  Zero: Double;
begin
  WriteLn('Create(array of TPair) and Create(Source)');
  D := TDictionary<string, Integer>.Create([TPair<string, Integer>.Create('b', 2),
    TPair<string, Integer>.Create('a', 1)]);
  D2 := TDictionary<string, Integer>.Create(D);
  D2.Add('c', 3);
  for P in D2 do
    WriteLn('  ', P.Key, '=', P.Value);
  WriteLn('  ', D.Count, ' ', D2.Count);
  D2.Free;
  D.Free;

  WriteLn('ContainsValue uses key equality');
  Zero := 0.0;
  F := TDictionary<string, Double>.Create;
  F.Add('nan', Zero / Zero);
  F.Add('zero', -0.0);
  WriteLn('  ', F.ContainsValue(Zero / Zero), ' ', F.ContainsValue(0.0), ' ',
    F.ContainsValue(1.0));
  F.Free;

  WriteLn('owning dictionary with a comparer');
  OD := TObjectDictionary<string, TItem>.Create([TDictionaryOwnership.doOwnsValues],
    TStringComparer.OrdinalIgnoreCase);
  OD.Add('Key', TItem.Create('v1'));
  OD['KEY'] := TItem.Create('v2');
  WriteLn('  ', OD.Count, ' ', OD['key'].Name);
  OD.Free;
  OD := TObjectDictionary<string, TItem>.Create([TDictionaryOwnership.doOwnsValues], 8,
    TStringComparer.OrdinalIgnoreCase);
  OD.Add('Key', TItem.Create('v3'));
  WriteLn('  ', OD.ContainsKey('kEY'), ' ', OD.Capacity >= 8);
  OD.Free;

  WriteLn('inherited Create in a subclass is checked at run time');
  Sub := nil;
  try
    Sub := TOwnStringKeys.Create;
  except
    on E: EInvalidOperation do WriteLn('  ', E.Message);
  end;
  WriteLn('  ', Sub = nil);
end.
