// Where the attributes of the testing framework can be written (attributes.md 5.1): E0785 for
// each kind of declaration that cannot take one. A second `[Test]` on one declaration is the same
// as one and is not reported again.
unit Test_attr_place_test;

interface

implementation

type
  [Test]
  TNotRoutine = class
    [Test]
    FField: Integer;
    [Test]
    constructor Create;
    [Test]
    class procedure ClassProc;
    [TestCase(1)]
    function Func: Integer;
    [Setup]
    class procedure ClassSetup;
    [Skip]
    procedure SkipOnly;
    [Setup]
    [Test]
    procedure Both;
    [Test]
    property P: Integer read FField;
  end;

  IFoo = interface
    [Test]
    procedure M;
  end;

  TRec = record
    [Test]
    procedure M;
  end;

  TGen<T> = class
    [Test]
    procedure M;
  end;

  TAbs = class
    [Test]
    procedure M; virtual; abstract;
  end;

constructor TNotRoutine.Create;
begin
end;

class procedure TNotRoutine.ClassProc;
begin
end;

function TNotRoutine.Func: Integer;
begin
  Result := 0;
end;

class procedure TNotRoutine.ClassSetup;
begin
end;

procedure TNotRoutine.SkipOnly;
begin
end;

procedure TNotRoutine.Both;
begin
end;

procedure TRec.M;
begin
end;

procedure TGen<T>.M;
begin
end;

[Test]
function F: Integer;
begin
  Result := 1;
end;

[Setup]
procedure NotAMethod;
begin
end;

[Test]
procedure G<T>;
begin
end;

[Skip('why')]
procedure SkipAlone;
begin
end;

type
  TOk = class
    [Test]
    [Test]
    procedure Twice;
  end;

procedure TOk.Twice;
begin
end;

procedure Local;
type
  TLocal = class
    [Test]
    procedure M;
  end;
procedure TLocal.M;
begin
end;
begin
end;

end.
