program GenericOwnedListFree;

uses
  System.SysUtils;

// A generic type whose type parameter takes only manually managed classes
// (`T: class, NotRefCounted`, or a class type constraint that is such a class) frees its values
// with `Free` and `FreeAndNil` (generics.md 3.3, 3.6): every element is destroyed once, when the
// list frees it, and nothing leaks. The concrete class's destructor runs (`Destroy` is virtual).

type
  TItem = class
    FName: string;
    constructor Create(const AName: string);
    destructor Destroy; override;
  end;

  TBigItem = class(TItem)
    destructor Destroy; override;
  end;

  TOwnedList<T: class, NotRefCounted> = class
  private
    FItems: TArray<T>;
  public
    procedure Add(const X: T);
    procedure DeleteLast;
    destructor Destroy; override;
  end;

  THolder<T: TItem> = class
    FItem: T;
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

destructor TBigItem.Destroy;
begin
  WriteLn('destroy big ', FName);
  inherited;
end;

procedure TOwnedList<T>.Add(const X: T);
begin
  FItems := FItems + [X];
end;

procedure TOwnedList<T>.DeleteLast;
var
  Last: T;
begin
  Last := FItems[High(FItems)];
  SetLength(FItems, Length(FItems) - 1);
  FreeAndNil(Last);
  WriteLn('deleted, assigned: ', Assigned(Last));
end;

destructor TOwnedList<T>.Destroy;
begin
  WriteLn('list destroyed with ', Length(FItems));
  for var X in FItems do
    X.Free;
  inherited;
end;

destructor THolder<T>.Destroy;
begin
  FreeAndNil(FItem);
  inherited;
end;

var
  Items: TOwnedList<TItem>;
  Objects: TOwnedList<TObject>;
  Holder: THolder<TBigItem>;
begin
  Items := TOwnedList<TItem>.Create;
  Items.Add(TItem.Create('a'));
  Items.Add(TBigItem.Create('b'));
  Items.Add(TItem.Create('c'));
  Items.DeleteLast;
  Items.Free;
  Objects := TOwnedList<TObject>.Create;
  Objects.Add(TItem.Create('in objects'));
  Objects.Add(TObject.Create);
  Objects.Free;
  Holder := THolder<TBigItem>.Create;
  Holder.FItem := TBigItem.Create('held');
  Holder.Free;
end.
