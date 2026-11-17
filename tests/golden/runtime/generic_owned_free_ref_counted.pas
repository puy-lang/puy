program GenericOwnedFreeRefCounted;

// `TObject` satisfies `NotRefCounted`, so `TOwnedList<TObject>` can hold an instance of a reference
// counted class (generics.md 3.3, 3.6). Its body frees the elements with `Free` of `TObject`, which
// raises `EInvalidOperation` at run time for a reference counted instance (objects.md 3.2).

type
  TItem = class
    FName: string;
    constructor Create(const AName: string);
    destructor Destroy; override;
  end;

  TCounted = class(TRefObject)
  end;

  TOwnedList<T: class, NotRefCounted> = class
    FItems: TArray<T>;
    procedure Add(const X: T);
    destructor Destroy; override;
  end;

constructor TItem.Create(const AName: string);
begin
  FName := AName;
end;

destructor TItem.Destroy;
begin
  WriteLn('destroy ', FName);
  inherited;
end;

procedure TOwnedList<T>.Add(const X: T);
begin
  FItems := FItems + [X];
end;

destructor TOwnedList<T>.Destroy;
begin
  for var X in FItems do
    X.Free;
  inherited;
end;

var
  L: TOwnedList<TObject>;
begin
  L := TOwnedList<TObject>.Create;
  L.Add(TItem.Create('first'));
  L.Add(TItem.Create('second'));
  L.Add(TCounted.Create);
  L.Free;
end.
