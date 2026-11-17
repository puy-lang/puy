program MethodRefRaise;

// A call through a method reference that raises (objects.md 14.9): the handler drops the last
// reference to its own instance and raises; the unwinding leaves the call's frame and lets go of
// the `Self` the call counted, so the instance is destroyed once, before the `except` block, and
// nothing leaks.

uses System.SysUtils;

type
  TEvent = procedure of object;

  TNode = class(TRefObject)
    destructor Destroy; override;
    procedure Fail;
  end;

var
  Keeper: TNode;

destructor TNode.Destroy;
begin
  WriteLn('destroy');
  inherited;
end;

procedure TNode.Fail;
begin
  Keeper := nil;
  WriteLn('raising');
  raise Exception.Create('failed');
end;

procedure Run;
var
  N: TNode;
  E: TEvent;
begin
  N := TNode.Create;
  Keeper := N;
  N := nil;
  E := Keeper.Fail;
  try
    E;
  except
    on X: Exception do WriteLn('caught: ', X.Message);
  end;
  WriteLn('assigned: ', Assigned(E));
end;

begin
  Run;
  WriteLn('end');
end.
