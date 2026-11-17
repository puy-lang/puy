program StdlibOwningDictionaryRefCounted;

// A TObjectDictionary whose owned K or V is exactly TObject refuses an instance of a reference
// counted class on that side when it goes in (collections.md 9.2, 9.3): Add, TryAdd, AddOrSetValue
// and D[K] := V (the entry already there keeps its value, and the key passed in is not freed),
// and the same calls through the parent type. The refused instance is destroyed when its last
// reference goes. A side that is not owned (Create without Ownerships) accepts it, and instances of manual classes are owned
// as usual.

uses System.SysUtils, System.Generics.Collections;

type
  TRef = class(TRefObject)
  public
    Name: string;
    constructor Create(const AName: string);
    destructor Destroy; override;
  end;

  TItem = class
  public
    Name: string;
    constructor Create(const AName: string);
    destructor Destroy; override;
  end;

constructor TRef.Create(const AName: string);
begin
  Name := AName;
end;

destructor TRef.Destroy;
begin
  WriteLn('destroy ', Name);
  inherited Destroy;
end;

constructor TItem.Create(const AName: string);
begin
  Name := AName;
end;

destructor TItem.Destroy;
begin
  WriteLn('free ', Name);
  inherited Destroy;
end;

procedure Report(E: Exception);
begin
  WriteLn(E.ClassName, ': ', E.Message);
end;

// The temporaries holding the refused instance are let go of at the end of the routine
procedure OwnedValues;
var
  D: TObjectDictionary<string, TObject>;
  P: TDictionary<string, TObject>;
  R: TRef;
begin
  D := TObjectDictionary<string, TObject>.Create([TDictionaryOwnership.doOwnsValues]);
  D.Add('a', TItem.Create('a'));
  R := TRef.Create('r1');
  try
    D.Add('r', R);
  except
    on E: EInvalidOperation do
      Report(E);
  end;
  try
    D.TryAdd('r', R);
  except
    on E: EInvalidOperation do
      Report(E);
  end;
  try
    D.AddOrSetValue('a', R);
  except
    on E: EInvalidOperation do
      Report(E);
  end;
  try
    D['a'] := R;
  except
    on E: EInvalidOperation do
      Report(E);
  end;
  P := D;
  try
    P.Add('r', R);
  except
    on E: EInvalidOperation do
      Report(E);
  end;
  WriteLn('count ', D.Count, ', a = ', TItem(D['a']).Name);
  D.Free;
end;

procedure OwnedKeys;
var
  K: TObjectDictionary<TObject, string>;
begin
  K := TObjectDictionary<TObject, string>.Create([TDictionaryOwnership.doOwnsKeys]);
  K.Add(TItem.Create('k'), 'item');
  try
    K.Add(TRef.Create('r2'), 'ref');
  except
    on E: EInvalidOperation do
      Report(E);
  end;
  WriteLn('count ', K.Count);
  K.Free;
end;

var
  D: TObjectDictionary<string, TObject>;

begin
  WriteLn('-- owned values');
  OwnedValues;
  WriteLn('-- owned keys');
  OwnedKeys;
  WriteLn('-- not owned');
  D := TObjectDictionary<string, TObject>.Create;
  D.Add('r', TRef.Create('r3'));
  WriteLn('count ', D.Count);
  D.Free;
  WriteLn('end');
end.
