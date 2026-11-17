// What anonymous methods capture (objects.md 9.6, 14.9): a variable of the script's top level
// (a local of `<main>`), a variable of the routine a nested routine is in (reached through the
// static link, then captured), a `[weak]` variable (still weak: it reads `nil` once the instance
// is gone), `E` of `on E` (kept after the handler ends), and a record whose fields the anonymous
// method writes.

uses System.SysUtils;

type
  TNode = class(TRefObject)
    Name: string;
    destructor Destroy; override;
  end;
  TPoint = record
    X, Y: Integer;
  end;

destructor TNode.Destroy;
begin
  WriteLn('destroy ', Name);
  inherited;
end;

procedure Outer;
var
  Count: Integer;
  procedure Inner;
  var
    P: TProc;
  begin
    P := procedure begin Inc(Count); WriteLn('count ', Count) end;
    P();
    P();
  end;
begin
  Count := 5;
  Inner;
  WriteLn('after ', Count);
end;

procedure Weak;
var
  N: TNode;
  [weak] W: TNode;
  P: TProc;
begin
  N := TNode.Create;
  N.Name := 'n';
  W := N;
  P := procedure begin WriteLn(Assigned(W)) end;
  P();
  N := nil;
  P();
end;

procedure Exc;
var
  P: TProc;
begin
  P := nil;
  try
    raise Exception.Create('boom');
  except
    on E: Exception do
      P := procedure begin WriteLn('caught ', E.Message) end;
  end;
  P();
end;

procedure Rec;
var
  R: TPoint;
  P: TProc;
begin
  R := Default(TPoint);
  R.X := 1;
  R.Y := 2;
  P := procedure begin R.X := R.X + 10; Inc(R.Y) end;
  P();
  WriteLn(R.X, ' ', R.Y);
end;

var Top := 'top';
var Show: TProc := procedure begin WriteLn('script ', Top) end;
Top := 'changed';
Show();
Outer;
Weak;
Exc;
Rec;
