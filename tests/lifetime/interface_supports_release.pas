program InterfaceSupportsRelease;

// Supports and `as` with interfaces keep no counted reference of their own (objects.md 7.6):
// Supports of an instance whose only reference is a variable of its class does not destroy it,
// Supports writes its `out` variable (the new value, or `nil` over the instance it reads, which
// the temporary holding the first argument keeps until the routine ends; design/lifetime.md 3.4),
// an instance made in the argument is destroyed when the routine ends, and a failed `as` leaves
// the counts as they were. An instance made in a value argument is held by the caller's
// temporary (design/lifetime.md 3.3), so each such call is in a routine of its own.

uses System.SysUtils;

type
  IFoo = interface
    function Name: string;
  end;

  IBar = interface
    procedure Bar;
  end;

  TNode = class(TRefObject, IFoo)
  private
    FName: string;
  public
    constructor Create(const AName: string);
    destructor Destroy; override;
    function Name: string;
  end;

constructor TNode.Create(const AName: string);
begin
  inherited Create;
  FName := AName;
end;

destructor TNode.Destroy;
begin
  WriteLn('destroy ', FName);
  inherited Destroy;
end;

function TNode.Name: string;
begin
  Result := FName;
end;

procedure KeepsInstance;
var
  N: TNode;
begin
  N := TNode.Create('a');
  WriteLn(Supports(N, IFoo), ' ', Supports(N, IBar));
  WriteLn('a is alive: ', N.Name);
end;

procedure ReleasesOldValue(F: IFoo);
var
  G: IFoo;
  U: IInterface;
begin
  G := F;
  U := G;
  G := nil;
  WriteLn(Supports(U, IFoo, G), ' ', G.Name);
  U := nil;
  F := nil;
  WriteLn('still held by G');
  U := G;
  G := nil;
  WriteLn(Supports(U, IBar, U), ' ', U = nil);
  WriteLn('after the last reference');
end;

procedure MadeInArgument;
begin
  WriteLn(Supports(TNode.Create('c'), IFoo));
end;

procedure FailedAs(F: IFoo);
var
  B: IBar;
begin
  try
    B := F as IBar;
  except
    on E: EInvalidCast do
      WriteLn('as failed');
  end;
end;

procedure PassesB;
begin
  ReleasesOldValue(TNode.Create('b'));
end;

procedure PassesD;
begin
  FailedAs(TNode.Create('d'));
end;

begin
  KeepsInstance;
  WriteLn('-');
  PassesB;
  WriteLn('-');
  MadeInArgument;
  WriteLn('after c');
  PassesD;
  WriteLn('end');
end.
