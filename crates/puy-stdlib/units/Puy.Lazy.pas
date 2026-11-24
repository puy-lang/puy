unit Puy.Lazy;

// TLazy<T>, a value made by a function the first time it is read, and TLazyObject<T>, which
// also frees the object it made (docs/spec/stdlib/Puy.Lazy/README.md). Global variables belong
// to one isolate, so there is no lock: the value is made at most once per instance and isolate.

interface

uses
  System.SysUtils;

type
  TLazy<T> = class(TRefObject)
  private
    FFactory: TFunc<T>;
    FValue: T;
    FCreated: Boolean;
    FCreating: Boolean;
    function GetValue: T;
  protected
    // Lets go of the value that was made (called after FCreated is set to False).
    procedure ReleaseValue; virtual;
  public
    constructor Create(const Factory: TFunc<T>);
    constructor CreateValue(const AValue: T);
    procedure Reset;
    property Value: T read GetValue;
    property IsValueCreated: Boolean read FCreated;
  end;

  TLazyObject<T: class, NotRefCounted> = class(TLazy<T>)
  protected
    procedure ReleaseValue; override;
  public
    destructor Destroy; override;
  end;

implementation

{ TLazy<T> }

constructor TLazy<T>.Create(const Factory: TFunc<T>);
begin
  inherited Create;
  if not Assigned(Factory) then
    raise EArgumentException.Create('the function that makes the value of a TLazy is nil');
  FFactory := Factory;
end;

constructor TLazy<T>.CreateValue(const AValue: T);
begin
  inherited Create;
  FValue := AValue;
  FCreated := True;
end;

function TLazy<T>.GetValue: T;
begin
  if not FCreated then
  begin
    if FCreating then
      raise EInvalidOperation.Create('the value of a TLazy was read while it was being made');
    if not Assigned(FFactory) then
      raise EInvalidOperation.Create('a TLazy made by CreateValue cannot make its value again after Reset');
    FCreating := True;
    try
      // On an exception nothing is kept: the next read calls the function again.
      FValue := FFactory();
      FCreated := True;
    finally
      FCreating := False;
    end;
  end;
  Result := FValue;
end;

procedure TLazy<T>.ReleaseValue;
begin
  FValue := Default(T);
end;

procedure TLazy<T>.Reset;
begin
  if FCreating then
    raise EInvalidOperation.Create('a TLazy cannot be reset while its value is being made');
  if FCreated then
  begin
    FCreated := False;
    ReleaseValue;
  end;
end;

{ TLazyObject<T> }

procedure TLazyObject<T>.ReleaseValue;
var
  Obj: T;
begin
  // Clear the field first, so that a destructor that raises leaves no freed object behind
  Obj := FValue;
  FValue := nil;
  Obj.Free;
end;

destructor TLazyObject<T>.Destroy;
begin
  if FCreated then
  begin
    FCreated := False;
    ReleaseValue;
  end;
  inherited Destroy;
end;

end.
