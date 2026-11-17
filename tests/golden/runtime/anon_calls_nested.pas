program AnonCallsNested;

// An anonymous method can call the nested routines of the routines around it (objects.md 9.7):
// they are compiled as closures, so the call works after the routine they are nested in returns.
// Also: an anonymous method in such a routine calling a routine of the same group, a routine
// nested in another nested routine calling them through its static link, and a nested routine
// of a method using `Self`.

type
  TGreeter = class(TRefObject)
    Name: string;
    function Greeter: TProc;
  end;

function TGreeter.Greeter: TProc;
  procedure Hello;
  begin
    WriteLn('hello from ', Name);
  end;
begin
  Result := procedure
    begin
      Hello;
      Hello;
    end;
end;

function Collect: TProc;
var
  Log: string;

  procedure Add(const S: string);
  begin
    Log := Log + S + ';';
  end;

  procedure Show;
  begin
    WriteLn(Log);
  end;

  // Calls `Add` through the static link: it stays an ordinary nested routine
  procedure AddTwice(const S: string);
  begin
    Add(S);
    Add(S);
  end;

  function Later(const S: string): TProc;
  begin
    // `Add` and `Show` are of other groups; `Later` takes the block of each
    Result := procedure
      begin
        Add(S);
        Show;
      end;
  end;

begin
  Log := '';
  AddTwice('a');
  Later('b')();
  Result := Later('c');
end;

function Countdown(N: Integer): TProc;
  procedure Step(K: Integer);
  var
    Next: TProc;
  begin
    Write(K, ' ');
    if K = 0 then
    begin
      WriteLn;
      Exit;
    end;
    // An anonymous method in `Step` calling `Step` uses the block `Step` was called with
    Next := procedure
      begin
        Step(K - 1);
      end;
    Next();
  end;
begin
  Result := procedure
    begin
      Step(N);
    end;
end;

var
  G: TGreeter;
  P: TProc;
begin
  G := TGreeter.Create;
  G.Name := 'greeter';
  P := G.Greeter();
  G := nil;
  P();
  P := Collect();
  P();
  P := Countdown(3);
  P();
end.
