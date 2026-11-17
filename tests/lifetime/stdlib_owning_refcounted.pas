program StdlibOwningRefCounted;

// An owning container whose T is exactly TObject refuses an instance of a reference counted class
// when it is put in (collections.md 9.2): Add, Insert, AddRange (nothing is added when one is
// refused), L[I] := X (the old item stays), Push, Enqueue, the constructors taking items (no
// container is made, and the other items are neither taken nor freed), and the same calls
// through a variable of the parent type. The refused instance is destroyed when its last
// reference goes. While OwnsObjects is False it is accepted; setting OwnsObjects to True then
// raises and leaves it False. Instances of manual classes are accepted and owned as usual.

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

var
  L: TObjectList<TObject>;
  P: TList<TObject>;
  S: TObjectStack<TObject>;
  Q: TObjectQueue<TObject>;
  R: TRef;
  B: TItem;

// The temporary is let go of at the end of this routine (the instance is not kept anywhere)
procedure AddTemporary;
var
  L: TObjectList<TObject>;
begin
  L := TObjectList<TObject>.Create;
  try
    L.Add(TRef.Create('temp'));
  except
    on E: Exception do
      Report(E);
  end;
  L.Free;
end;

begin
  R := TRef.Create('r');
  L := TObjectList<TObject>.Create;
  L.Add(TItem.Create('a'));
  try
    L.Add(R);
  except
    on E: Exception do
      Report(E);
  end;
  try
    L.Insert(0, R);
  except
    on E: Exception do
      Report(E);
  end;
  B := TItem.Create('b');
  try
    L.AddRange([B, R]);
  except
    on E: Exception do
      Report(E);
  end;
  B.Free;
  try
    L[0] := R;
  except
    on E: Exception do
      Report(E);
  end;
  WriteLn('count ', L.Count, ', first ', L[0].ClassName);
  P := L;
  try
    P.Add(R);
  except
    on E: Exception do
      Report(E);
  end;
  WriteLn('count ', P.Count);
  L.Free;

  try
    L := TObjectList<TObject>.Create([R]);
  except
    on E: Exception do
      Report(E);
  end;

  WriteLn('-- stack and queue');
  // The constructors ask about every item first: B is not taken (nor freed) when R is refused
  B := TItem.Create('b');
  try
    S := TObjectStack<TObject>.Create([B, R]);
  except
    on E: Exception do
      Report(E);
  end;
  try
    Q := TObjectQueue<TObject>.Create([B, R]);
  except
    on E: Exception do
      Report(E);
  end;
  WriteLn('still ', B.Name);
  B.Free;
  S := TObjectStack<TObject>.Create;
  try
    S.Push(R);
  except
    on E: Exception do
      Report(E);
  end;
  WriteLn('count ', S.Count);
  S.Free;
  Q := TObjectQueue<TObject>.Create;
  try
    Q.Enqueue(R);
  except
    on E: Exception do
      Report(E);
  end;
  WriteLn('count ', Q.Count);
  Q.Free;

  WriteLn('-- not owning');
  L := TObjectList<TObject>.Create(False);
  L.Add(TItem.Create('c'));
  L.Add(R);
  try
    L.OwnsObjects := True;
  except
    on E: Exception do
      Report(E);
  end;
  WriteLn('owns ', L.OwnsObjects, ', count ', L.Count);
  L.Delete(1);
  L.OwnsObjects := True;
  WriteLn('owns ', L.OwnsObjects);
  L.Free;

  WriteLn('-- a refused temporary');
  AddTemporary;
  WriteLn('end');
end.
